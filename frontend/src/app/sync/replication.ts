import { WritableSignal } from '@angular/core';
import { RxCollection } from 'rxdb';
import { replicateRxCollection } from 'rxdb/plugins/replication';
import { EMPTY, Observable, fromEvent, interval, map, merge } from 'rxjs';

import { assertNever, classifyFetchResponse } from '../shared/api-error';
import { PULL_INTERVAL_MS } from './cadence';
import { isRecord, numberField } from '../shared/narrow';
import { SyncSource, SyncStatus } from './sync-status';

/** A document as it travels over sync: the local doc plus RxDB's tombstone. */
export type Synced<T> = T & { _deleted: boolean };

/** Throws on a lost session, after reporting it; offline and server failures
 *  return, and the caller's own throw retries them. */
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

/** The HTTP pull/push replication every synced collection uses (see
 *  docs/design/sync.md). */
export function startHttpReplication<T>(opts: {
  collection: RxCollection<T>;
  /** Stable RxDB replication identity, e.g. 'shopping-http-sync'. */
  identifier: string;
  /** Sync endpoint, e.g. '/api/sync/shopping'. */
  path: string;
  syncError: WritableSignal<string | null>;
  syncStatus: SyncStatus;
  label: SyncSource;
  /** Replication stops after calling this: only a fresh login helps. */
  onAuthLost: () => void;
  /** For tests; the app uses PULL_INTERVAL_MS. */
  pollMs?: number;
}) {
  let authLost = false;

  // Without `pull.stream$`, `live: true` pulls once and the tab freezes.
  // Keep `waitForLeadership` (the default): only the leader tab replicates.
  const heartbeat$: Observable<'RESYNC'> = merge(
    interval(opts.pollMs ?? PULL_INTERVAL_MS),
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
        // Rows are trusted; the two shapes RxDB cannot survive are checked (a
        // missing rev would rewind every pull to 0).
        const body: unknown = await res.json();
        const documents =
          isRecord(body) && Array.isArray(body['documents']) ? body['documents'] : null;
        const rev = numberField(isRecord(body) ? body['checkpoint'] : null, 'rev');
        if (documents === null || rev === null) throw new Error('pull returned a malformed batch');
        opts.syncError.set(null);
        opts.syncStatus.clearError(opts.label);
        // A stall throws nothing, so only a reported success shows we are current.
        opts.syncStatus.reportSuccess(opts.label);
        // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- the pull rows carry _deleted
        return { documents: documents as Synced<T>[], checkpoint: { rev } };
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
        const conflicts: unknown = await res.json();
        if (!Array.isArray(conflicts)) throw new Error('push returned a malformed response');
        // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- checked to be an array above
        return conflicts as Synced<T>[];
      },
    },
  });
  replication.error$.subscribe((err) => {
    const message =
      opts.syncError() ??
      'Can’t reach the server — changes are saved on this device and will sync when it’s back.';
    opts.syncStatus.reportError(opts.label, message);
    if (authLost) {
      // RxDB would retry every 5s forever, certain to fail the same way.
      opts.onAuthLost();
      void replication.cancel();
      return;
    }
    console.warn(`[${opts.label}]`, err);
  });
  return replication;
}
