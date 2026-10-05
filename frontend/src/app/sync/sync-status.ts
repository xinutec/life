import { Injectable, computed, signal } from '@angular/core';

import { STALE_AFTER_MS } from './cadence';

/** Whole-app sync health. Offline outranks error: a failed fetch is what
 *  offline looks like. */
export type SyncHealth = 'synced' | 'offline' | 'error' | 'stale';

/** Every replication that reports here. Closed, because it keys the status
 *  maps and a typo would leave an error never cleared. */
export type SyncSource = 'shopping sync' | 'todo sync' | 'todo-link sync' | 'wellbeing sync';

/** Lets a success age: `health()` is a computed and needs a moving `now`. */
const TICK_MS = 30_000;

/** Whether local edits reached the server; IndexedDB looks saved either way.
 *  Errors are kept per source, so one recovering cannot hide another failing. */
@Injectable({ providedIn: 'root' })
export class SyncStatus {
  private readonly online = signal(typeof navigator === 'undefined' ? true : navigator.onLine);
  private readonly errors = signal<Partial<Record<SyncSource, string>>>({});
  private readonly lastOk = signal<Partial<Record<SyncSource, number>>>({});
  private readonly now = signal(Date.now());

  constructor() {
    if (typeof window !== 'undefined') {
      window.addEventListener('online', () => this.online.set(true));
      window.addEventListener('offline', () => this.online.set(false));
      setInterval(() => this.refresh(), TICK_MS);
    }
  }

  /** Tests pass the instant they mean. */
  refresh(at: number = Date.now()): void {
    this.now.set(at);
  }

  /** Not the same as `clearError`: a stall never fails, so only a success
   *  shows the data is current. */
  reportSuccess(source: SyncSource, at: number = Date.now()): void {
    this.lastOk.update((m) => ({ ...m, [source]: at }));
    this.now.set(at);
  }

  /** The NEWEST success: keyed on the oldest, one quiet store would keep the
   *  whole app warning forever. */
  private readonly freshest = computed<number | null>(() => {
    const times = Object.values(this.lastOk()).filter((t): t is number => t !== undefined);
    return times.length ? Math.max(...times) : null;
  });

  /** Minutes since the newest success, or null while fresh. */
  private readonly staleMinutes = computed<number | null>(() => {
    const last = this.freshest();
    if (last === null) return null;
    const age = this.now() - last;
    if (age <= STALE_AFTER_MS) return null;
    // Never "0 minutes", which would reassure while the icon warns.
    return Math.max(1, Math.floor(age / 60_000));
  });

  readonly health = computed<SyncHealth>(() => {
    if (!this.online()) return 'offline';
    if (Object.keys(this.errors()).length > 0) return 'error';
    return this.staleMinutes() === null ? 'synced' : 'stale';
  });

  /** A total Record, so a new state must name its glyph. */
  readonly icon = computed<string>(
    () =>
      ({
        synced: 'cloud_done',
        offline: 'cloud_off',
        error: 'sync_problem',
        stale: 'history',
      })[this.health()],
  );

  /** Tooltip and aria-label. */
  readonly message = computed<string>(() => {
    if (!this.online()) {
      return 'Offline — changes are saved on this device and will sync when you reconnect.';
    }
    const [first] = Object.values(this.errors()).filter((m): m is string => m !== undefined);
    if (first) return first;
    const mins = this.staleMinutes();
    if (mins !== null) {
      return `Nothing has synced for ${mins} minutes — this device may be showing old data.`;
    }
    return 'All changes synced.';
  });

  reportError(source: SyncSource, message: string): void {
    this.errors.update((e) => (e[source] === message ? e : { ...e, [source]: message }));
  }

  clearError(source: SyncSource): void {
    this.errors.update((e) => {
      if (!(source in e)) return e;
      const next = { ...e };
      delete next[source];
      return next;
    });
  }
}
