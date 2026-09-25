import { signal } from '@angular/core';
import { createRxDatabase, type RxCollection, type RxJsonSchema } from 'rxdb';
import { replicateRxCollection } from 'rxdb/plugins/replication';
import { getRxStorageMemory } from 'rxdb/plugins/storage-memory';
import { describe, expect, it, vi } from 'vitest';

import { AuthState } from './auth-state';
import { guardAuth, startHttpReplication } from './replication';
import { SyncStatus } from './sync-status';

/** Minimal Response stand-in — guardAuth classifies status/ok/redirected/headers. */
function res(over: { status?: number; contentType?: string | null; redirected?: boolean }): Response {
  const status = over.status ?? 200;
  return {
    status,
    ok: status >= 200 && status < 300,
    redirected: over.redirected ?? false,
    headers: new Headers(over.contentType === null ? {} : { 'content-type': over.contentType ?? 'application/json' }),
  } as Response;
}

describe('guardAuth — expired-session detection on sync fetches', () => {
  it('lets a healthy JSON response through and leaves the error signal alone', () => {
    const err = signal<string | null>(null);
    const lost = vi.fn();
    expect(() => guardAuth(res({}), err, lost)).not.toThrow();
    expect(err()).toBeNull();
    expect(lost).not.toHaveBeenCalled();
  });

  it('flags 401 and 403 as login-required', () => {
    for (const status of [401, 403]) {
      const err = signal<string | null>(null);
      expect(() => guardAuth(res({ status }), err)).toThrow('auth-required');
      expect(err()).toContain('login required');
    }
  });

  it('tells the caller the session is gone, so replication can stand down', () => {
    // A 401 is not transient: retrying it forever neither recovers nor
    // informs, so the guard must report the loss rather than just throw.
    for (const status of [401, 403]) {
      const lost = vi.fn();
      expect(() => guardAuth(res({ status }), signal<string | null>(null), lost)).toThrow();
      expect(lost).toHaveBeenCalledOnce();
    }
    // A redirect to the login page is the same story, wearing a 200.
    const lost = vi.fn();
    expect(() =>
      guardAuth(res({ redirected: true, contentType: 'text/html' }), signal<string | null>(null), lost),
    ).toThrow();
    expect(lost).toHaveBeenCalledOnce();
  });

  it('does NOT report the session lost for a plain server error', () => {
    // A 500 IS transient — retrying is right, and flipping the app to signed-out
    // would throw the user onto a login page over a blip.
    const lost = vi.fn();
    expect(() => guardAuth(res({ status: 500 }), signal<string | null>(null), lost)).not.toThrow();
    expect(lost).not.toHaveBeenCalled();
  });

  it('flags a followed redirect — the stale-cookie 302→login-page→200 case', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ redirected: true, contentType: 'text/html' }), err)).toThrow('auth-required');
    expect(err()).toContain('login required');
  });

  it('flags a non-JSON body even on 200 — HTML where JSON was expected', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ contentType: 'text/html' }), err)).toThrow('auth-required');
  });

  it('flags a missing content-type on a 200', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ contentType: null }), err)).toThrow('auth-required');
  });

  it('does NOT sign the user out on the service worker’s offline 504', () => {
    // ngsw answers a failed fetch with a bodiless synthetic 504 (no
    // content-type). Offline is never proof of auth loss; the cycle must retry
    // quietly.
    const err = signal<string | null>(null);
    const lost = vi.fn();
    expect(() => guardAuth(res({ status: 504, contentType: null }), err, lost)).not.toThrow();
    expect(err()).toBeNull();
    expect(lost).not.toHaveBeenCalled();
  });

  it('does NOT sign the user out on an ingress HTML error page', () => {
    // Backend down: traefik serves its own HTML 502/503. Same story as the 504 —
    // a non-ok non-JSON response is a broken server, not a logged-out session.
    for (const status of [502, 503]) {
      const lost = vi.fn();
      expect(() =>
        guardAuth(res({ status, contentType: 'text/html' }), signal<string | null>(null), lost),
      ).not.toThrow();
      expect(lost).not.toHaveBeenCalled();
    }
  });

  it('does NOT flag a JSON error status like 500 — that is a plain retry, not auth', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ status: 500 }), err)).not.toThrow();
    expect(err()).toBeNull();
  });
});

describe('AuthState', () => {
  it('starts hopeful and latches once the session is gone', () => {
    const auth = new AuthState();
    expect(auth.lost()).toBe(false);
    auth.lose();
    expect(auth.lost()).toBe(true);
    // Idempotent: every collection's replication reports the same 401, and the
    // shell must not thrash between states because three stores each said so.
    auth.lose();
    expect(auth.lost()).toBe(true);
  });
});

/** The pull must keep going on its own.
 *
 *  `live: true` is not enough: without a pull stream RxDB performs the FIRST
 *  pull and nothing after it, so an open tab silently freezes at whatever the
 *  server held when it loaded. This asserts a SECOND pull happens unprompted. */
describe('startHttpReplication — the pull keeps going', () => {
  async function harness(pollMs: number) {
    // ast-grep-ignore: life-single-rxdb
    const db = await createRxDatabase({
      name: `poll-spec-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      storage: getRxStorageMemory(),
      // Leadership gates `start()` only when the database is multi-instance;
      // off here so this test measures the heartbeat and not the election.
      multiInstance: false,
    });
    const added = await db.addCollections({
      entries: {
        schema: {
          version: 0,
          primaryKey: 'ulid',
          type: 'object',
          properties: { ulid: { type: 'string', maxLength: 26 }, rev: { type: 'number' } },
          required: ['ulid', 'rev'],
        } as RxJsonSchema<{ ulid: string; rev: number }>,
      },
    });
    const pulls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) => {
        pulls.push(String(url));
        return Promise.resolve(
          new Response(JSON.stringify({ documents: [], checkpoint: { rev: 0 } }), {
            headers: { 'content-type': 'application/json' },
          }),
        );
      }),
    );
    const replication = startHttpReplication({
      collection: added.entries as RxCollection<{ ulid: string; rev: number }>,
      identifier: 'poll-spec-sync',
      path: '/api/sync/entries',
      syncError: signal<string | null>(null),
      // ⚠ The REAL SyncStatus: a partial stub cast to it fails at runtime, not
      // at compile time, when a method is added.
      syncStatus: new SyncStatus(),
      label: 'wellbeing sync',
      onAuthLost: () => {},
      pollMs,
    });
    await replication.awaitInitialReplication();
    return { db, pulls, replication };
  }

  it('pulls again on its own after the first cycle', async () => {
    const { db, pulls, replication } = await harness(40);
    expect(pulls).toHaveLength(1);
    await vi.waitFor(() => expect(pulls.length).toBeGreaterThanOrEqual(3), { timeout: 2000 });
    expect(pulls.every((u) => u.startsWith('/api/sync/entries?since='))).toBe(true);
    await replication.cancel();
    await db.close();
  });

  it('waits for the interval rather than spinning', async () => {
    // The other half of the same change: a stream that emitted immediately, or
    // on every tick of something faster than it claims, would fix the freeze by
    // hammering the server instead. One request per interval, no more.
    const { db, pulls, replication } = await harness(10_000);
    await new Promise((r) => setTimeout(r, 300));
    expect(pulls).toHaveLength(1);
    await replication.cancel();
    await db.close();
  });
});

/** The rxdb semantics `SyncedStore.reSync` depends on.
 *
 *  `reSync()` emits `'RESYNC'` into a plain Subject that only the INTERNAL
 *  replication subscribes to, and that object does not exist until `start()`.
 *  So in a non-leader tab, whose replication never started, a `reSync()` is
 *  silently dropped — and a row restored from the Trash never comes back.
 *
 *  `start()` covers both cases — `_start` re-syncs when `wasStarted` and begins
 *  the replication otherwise — which is why the store calls that instead. This
 *  test pins the library behaviour the choice rests on, so an rxdb upgrade that
 *  changes it fails here rather than in the Trash page.
 *
 *  `autoStart: false` stands in for "waiting for leadership": in both cases the
 *  replication object exists and has never run. */
describe('rxdb: reSync on a replication that never started', () => {
  async function parked() {
    // ast-grep-ignore: life-single-rxdb
    const db = await createRxDatabase({
      name: `parked-spec-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      storage: getRxStorageMemory(),
      multiInstance: false,
    });
    const added = await db.addCollections({
      entries: {
        schema: {
          version: 0,
          primaryKey: 'ulid',
          type: 'object',
          properties: { ulid: { type: 'string', maxLength: 26 }, rev: { type: 'number' } },
          required: ['ulid', 'rev'],
        } as RxJsonSchema<{ ulid: string; rev: number }>,
      },
    });
    const pulls: number[] = [];
    const replication = replicateRxCollection<{ ulid: string; rev: number }, { rev: number }>({
      collection: added.entries as RxCollection<{ ulid: string; rev: number }>,
      replicationIdentifier: 'parked-spec-sync',
      live: true,
      autoStart: false,
      pull: {
        handler: () => {
          pulls.push(Date.now());
          return Promise.resolve({ documents: [], checkpoint: { rev: 0 } });
        },
      },
    });
    return { db, pulls, replication };
  }

  it('drops the reSync silently', async () => {
    const { db, pulls, replication } = await parked();
    replication.reSync();
    await new Promise((r) => setTimeout(r, 200));
    expect(pulls).toHaveLength(0);
    await replication.cancel();
    await db.close();
  });

  it('but honours start(), which is why the store calls that', async () => {
    const { db, pulls, replication } = await parked();
    await replication.start();
    await vi.waitFor(() => expect(pulls.length).toBeGreaterThanOrEqual(1), { timeout: 2000 });
    await replication.cancel();
    await db.close();
  });
});
