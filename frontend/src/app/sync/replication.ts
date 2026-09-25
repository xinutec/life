import { WritableSignal } from '@angular/core';
import { RxCollection } from 'rxdb';
import { replicateRxCollection } from 'rxdb/plugins/replication';
import { EMPTY, Observable, fromEvent, interval, map, merge } from 'rxjs';

import { assertNever, classifyFetchResponse } from '../shared/api-error';
import { PULL_INTERVAL_MS } from './cadence';
import { isRecord, numberField } from '../shared/narrow';
import { SyncSource, SyncStatus } from './sync-status';

/** Auth guard for sync fetches. An expired session shows up two ways: our API
 *  returns 401/403 JSON, or a stale cookie 302-redirects to a login page that
 *  fetch follows to a 200 non-JSON body. Either way, surface "login required",
 *  tell the caller the session is gone, and throw so the cycle aborts without
 *  corrupting the queue. Must run BEFORE the generic !res.ok check so this
 *  friendly message wins over "pull failed: 401".
 *
 *  What counts as auth loss is decided by `classifyFetchResponse` — the shared
 *  boundary, NOT re-derived here. This guard once read status/content-type
 *  inline and treated every non-JSON response as "logged out", which signed
 *  users out on the service worker's offline 504 and wiped their cached
 *  identity (2026-07-16). An offline/server failure returns normally so the
 *  caller's generic !res.ok throw retries it quietly. Pure(ish) and exported
 *  so the branching is unit-testable. */
export function guardAuth(
  res: Response,
  syncError: WritableSignal<string | null>,
  onAuthLost?: () => void,
): void {
  const f = classifyFetchResponse(res);
  switch (f.kind) {
    case 'ok':
    case 'offline':
    case 'server':
      return;
    case 'unauthenticated':
      syncError.set('login required — reopen the app to sign in');
      onAuthLost?.();
      throw new Error('auth-required');
    default:
      assertNever(f);
  }
}

/** Start the standard HTTP pull/push replication every synced collection uses
 *  (see docs/design/sync.md). One implementation instead of three
 *  copies — the shape is identical per collection: GET `path?since&limit` for
 *  pulls, POST `path` with the RxDB rows for pushes, rev-checkpointing, the
 *  auth guard, and quiet retry on transient errors. */
export function startHttpReplication<T>(opts: {
  collection: RxCollection<T>;
  /** Stable RxDB replication identity, e.g. 'shopping-http-sync'. */
  identifier: string;
  /** Sync endpoint, e.g. '/api/sync/shopping'. */
  path: string;
  /** The owning store's user-facing sync problem signal. */
  syncError: WritableSignal<string | null>;
  /** App-wide sync-health aggregator — every cycle reports success/failure here
   *  so the shell can show a persistent "not synced" indicator. */
  syncStatus: SyncStatus;
  /** console.warn tag + sync-status source key. A closed union, because it
   *  keys the status maps — see `SyncSource`. */
  label: SyncSource;
  /** Raised once the server refuses us for want of a session. Replication then
   *  STOPS: a 401 is not a transient error, and retrying it on a timer forever
   *  neither recovers nor informs — it just burns a request every few seconds
   *  until the tab is closed. Only a fresh login can help, so we say so and
   *  stand down. */
  onAuthLost: () => void;
  /** How often to ask the server whether anything is new, in milliseconds.
   *  Overridable only so tests need not wait a minute; nothing in the app sets
   *  it. */
  pollMs?: number;
}) {
  // Set by the guard inside a handler; read in error$, which is where we have the
  // replication object to cancel.
  let authLost = false;

  // ⚠ **`live: true` DOES NOT MEAN "keeps pulling", and this is the whole of
  // #1567.** RxDB subscribes to an ongoing pull only under
  // `if (this.pull && this.pull.stream$ && this.live)` — with no stream it
  // performs the first pull and then nothing, bar a local write or an explicit
  // `reSync()`. A tab left open froze at whatever the server held when it
  // loaded: on 2026-09-12 the browser had 248 of the server's 249 check-ins and
  // drew a calendar with no square for the current day, while the phone — a
  // single WebView, so always its own leader and started that morning — had it.
  // It fails silently and read-only, so nothing on screen tells the two apart.
  //
  // `reSync()` emits into `remoteEvents$`, which IS the internal
  // `masterChangeStream$`, so a stream of 'RESYNC' is the mechanism the plugin
  // intends rather than a timer bolted alongside it.
  //
  // ⚠ **`waitForLeadership` stays at its default of true.** With
  // `multiInstance: true` that gates `start()` on winning the election, which
  // is why a second tab pulls nothing at all — but leadership is also what
  // stops N tabs pushing the same local changes at one endpoint. Curing a pull
  // problem by turning it off would buy a push problem, paid for in the
  // conflict handler. The leader polls and the shared IndexedDB carries the
  // result to every tab.
  const heartbeat$: Observable<'RESYNC'> = merge(
    interval(opts.pollMs ?? PULL_INTERVAL_MS),
    // Coming back from offline should not wait out the rest of the interval.
    typeof window === 'undefined' ? EMPTY : fromEvent(window, 'online'),
  ).pipe(map(() => 'RESYNC' as const));

  const replication = replicateRxCollection<T, { rev: number }>({
    collection: opts.collection,
    replicationIdentifier: opts.identifier,
    live: true,
    retryTime: 5000,
    pull: {
      batchSize: 200,
      stream$: heartbeat$,
      handler: async (checkpoint, batchSize) => {
        const since = checkpoint?.rev ?? 0;
        const res = await fetch(`${opts.path}?since=${since}&limit=${batchSize}`, {
          credentials: 'include',
        });
        guardAuth(res, opts.syncError, () => (authLost = true));
        if (!res.ok) throw new Error(`pull failed: ${res.status}`);
        // The row TYPE is our own wire contract and is taken on trust, but the
        // two things replication cannot survive being wrong about are checked:
        // a non-array `documents` would be fed to RxDB as a batch, and a
        // missing `checkpoint.rev` would rewind the pull to 0 and refetch
        // everything on every cycle.
        const body: unknown = await res.json();
        const documents = isRecord(body) && Array.isArray(body['documents']) ? body['documents'] : null;
        const rev = numberField(isRecord(body) ? body['checkpoint'] : null, 'rev');
        if (documents === null || rev === null) throw new Error('pull returned a malformed batch');
        opts.syncError.set(null);
        opts.syncStatus.clearError(opts.label);
        // ⚠ A cycle that SUCCEEDS has to say so, not merely fail to fail. A
        // stall throws nothing, so `clearError` had nothing to clear and the
        // indicator went on claiming "All changes synced." for a day (#1567).
        opts.syncStatus.reportSuccess(opts.label);
        // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- _deleted is what the pull rows carry; the type adds RxDB's flag to T
        return { documents: documents as (T & { _deleted: boolean })[], checkpoint: { rev } };
      },
    },
    push: {
      batchSize: 50,
      handler: async (rows) => {
        const res = await fetch(opts.path, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          credentials: 'include',
          body: JSON.stringify(rows),
        });
        guardAuth(res, opts.syncError, () => (authLost = true));
        if (!res.ok) throw new Error(`push failed: ${res.status}`);
        opts.syncError.set(null);
        opts.syncStatus.clearError(opts.label);
        opts.syncStatus.reportSuccess(opts.label);
        // The push response is the server's conflict list — same wire contract
        // as the pull rows, checked for being a list at all.
        const conflicts: unknown = await res.json();
        if (!Array.isArray(conflicts)) throw new Error('push returned a malformed response');
        // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- checked to be an array on the line above
        return conflicts as (T & { _deleted: boolean })[];
      },
    },
  });
  replication.error$.subscribe((err) => {
    // Surface every failed cycle to the app-wide indicator. The auth guard sets
    // a friendly syncError; for anything else (server down, 5xx, offline fetch)
    // use a reassuring generic — offline-first means the write is safe locally.
    const message =
      opts.syncError() ??
      'Can’t reach the server — changes are saved on this device and will sync when it’s back.';
    opts.syncStatus.reportError(opts.label, message);

    if (authLost) {
      // Stand down rather than retry. RxDB's retryTime would otherwise re-run
      // this handler every 5s for the life of the tab, and every attempt is
      // certain to fail the same way — no session, no recovery, no message. Tell
      // the app instead, so the shell can ask for a login.
      opts.onAuthLost();
      void replication.cancel();
      return;
    }

    // Keep the console breadcrumb only for the non-auth case (RxDB retries
    // transient network errors on its own).
    console.warn(`[${opts.label}]`, err);
  });
  return replication;
}
