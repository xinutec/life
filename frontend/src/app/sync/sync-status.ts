import { Injectable, computed, signal } from '@angular/core';

import { STALE_AFTER_MS } from './cadence';

/** Whole-app sync health, in priority order: a device that's offline reports
 *  `offline` even if replication is erroring, because a failed fetch is the
 *  expected symptom of being offline, not a fault to alarm about. */
export type SyncHealth = 'synced' | 'offline' | 'error' | 'stale';

/** Every replication that reports here, spelled once.
 *
 *  ⚠ **A closed union, not `string`, and the reason is that these are KEYS.**
 *  `reportError`, `clearError` and `reportSuccess` all index the same maps by
 *  this value, so one typo puts a success under a name whose error nobody ever
 *  clears — an indicator stuck in a state no cycle can leave, with nothing to
 *  see in review. Adding a synced collection now fails to compile until its
 *  label is added here, which is the point. */
export type SyncSource = 'shopping sync' | 'todo sync' | 'todo-link sync' | 'wellbeing sync';

/** How often the freshness question is re-asked. `health()` is a computed, so
 *  without something moving underneath it a success cannot age. */
const TICK_MS = 30_000;

/** The one place that knows whether local edits have actually reached the
 *  server — data in IndexedDB looks saved either way. Every replication
 *  reports its cycle outcome here; the shell renders a persistent
 *  indicator whenever health() isn't `synced`.
 *
 *  Kept dependency-free and push-updated (no polling): signals in, computed
 *  out. Errors are tracked per source (store label) so one failing collection
 *  doesn't mask another recovering. */
@Injectable({ providedIn: 'root' })
export class SyncStatus {
  /** navigator.onLine, kept live via the window online/offline events. */
  private readonly online = signal(typeof navigator === 'undefined' ? true : navigator.onLine);
  /** Latest failure message per replication source; empty object = all healthy. */
  private readonly errors = signal<Partial<Record<SyncSource, string>>>({});
  /** When each source last completed a cycle cleanly. Empty until one does. */
  private readonly lastOk = signal<Partial<Record<SyncSource, number>>>({});
  /** The moving `now` that lets a success age. Advanced by `refresh`. */
  private readonly now = signal(Date.now());

  constructor() {
    if (typeof window !== 'undefined') {
      window.addEventListener('online', () => this.online.set(true));
      window.addEventListener('offline', () => this.online.set(false));
      setInterval(() => this.refresh(), TICK_MS);
    }
  }

  /** Re-ask the freshness question. Driven by a timer in the browser; tests pass
   *  the instant they mean, so no assertion here depends on the wall clock. */
  refresh(at: number = Date.now()): void {
    this.now.set(at);
  }

  /** A replication cycle for `source` completed cleanly, at `at`.
   *
   *  ⚠ Distinct from `clearError`, which only says "the last failure is over":
   *  a stall never fails, so only a success can show the log is current. */
  reportSuccess(source: SyncSource, at: number = Date.now()): void {
    this.lastOk.update((m) => ({ ...m, [source]: at }));
    this.now.set(at);
  }

  /** The newest success across every source, or null before the first one.
   *
   *  ⚠ NEWEST, not oldest. Four collections replicate, and a quiet store lagging
   *  is not the app being stale; keyed on the oldest, a store nobody has written
   *  to would hold the whole app in a warning state for ever. */
  private readonly freshest = computed<number | null>(() => {
    // `Partial` is the honest shape — a map that starts empty does not have
    // every key — so the undefineds are filtered rather than asserted away.
    const times = Object.values(this.lastOk()).filter((t): t is number => t !== undefined);
    return times.length ? Math.max(...times) : null;
  });

  /** Minutes since the newest success, or null before the first one. */
  private readonly staleMinutes = computed<number | null>(() => {
    const last = this.freshest();
    if (last === null) return null;
    const age = this.now() - last;
    if (age <= STALE_AFTER_MS) return null;
    // ⚠ Never zero: "Nothing has synced for 0 minutes" reassures while the
    // icon warns. Unreachable at the five-minute threshold, until it is shortened.
    return Math.max(1, Math.floor(age / 60_000));
  });

  readonly health = computed<SyncHealth>(() => {
    if (!this.online()) return 'offline';
    if (Object.keys(this.errors()).length > 0) return 'error';
    return this.staleMinutes() === null ? 'synced' : 'stale';
  });

  /** The glyph each state shows, spelled out per state.
   *
   *  ⚠ **Exhaustive over `SyncHealth`**: a `Record` keyed on the union will not
   *  compile until a new state names its own icon, rather than inheriting one.
   *
   *  `history` for stale: not "something failed" but "this may be old". The
   *  error state keeps the alarming glyph and the only red. `synced` never
   *  draws — the indicator shows only when not synced — and is here to keep the
   *  map total. */
  readonly icon = computed<string>(
    () =>
      ({
        synced: 'cloud_done',
        offline: 'cloud_off',
        error: 'sync_problem',
        stale: 'history',
      })[this.health()],
  );

  /** A short human message for the current health — tooltip + aria-label. */
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

  /** A replication cycle failed. `source` is the store label; last message wins. */
  reportError(source: SyncSource, message: string): void {
    this.errors.update((e) => (e[source] === message ? e : { ...e, [source]: message }));
  }

  /** A replication cycle for `source` completed cleanly. */
  clearError(source: SyncSource): void {
    this.errors.update((e) => {
      if (!(source in e)) return e;
      const next = { ...e };
      delete next[source];
      return next;
    });
  }
}
