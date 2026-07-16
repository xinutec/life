import { WritableSignal } from '@angular/core';
import { RxCollection } from 'rxdb';
import { replicateRxCollection } from 'rxdb/plugins/replication';

import { assertNever, classifyFetchResponse } from '../shared/api-error';
import { SyncStatus } from './sync-status';

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
 *  (see docs/proposals/offline-first.md). One implementation instead of three
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
  /** console.warn tag + sync-status source key, e.g. 'shopping sync'. */
  label: string;
  /** Raised once the server refuses us for want of a session. Replication then
   *  STOPS: a 401 is not a transient error, and retrying it on a timer forever
   *  neither recovers nor informs — it just burns a request every few seconds
   *  until the tab is closed. Only a fresh login can help, so we say so and
   *  stand down. */
  onAuthLost: () => void;
}) {
  // Set by the guard inside a handler; read in error$, which is where we have the
  // replication object to cancel.
  let authLost = false;

  const replication = replicateRxCollection<T, { rev: number }>({
    collection: opts.collection,
    replicationIdentifier: opts.identifier,
    live: true,
    retryTime: 5000,
    pull: {
      batchSize: 200,
      handler: async (checkpoint, batchSize) => {
        const since = checkpoint?.rev ?? 0;
        const res = await fetch(`${opts.path}?since=${since}&limit=${batchSize}`, {
          credentials: 'include',
        });
        guardAuth(res, opts.syncError, () => (authLost = true));
        if (!res.ok) throw new Error(`pull failed: ${res.status}`);
        const body = (await res.json()) as {
          documents: (T & { _deleted: boolean })[];
          checkpoint: { rev: number };
        };
        opts.syncError.set(null);
        opts.syncStatus.clearError(opts.label);
        return { documents: body.documents, checkpoint: body.checkpoint };
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
        return (await res.json()) as (T & { _deleted: boolean })[];
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
