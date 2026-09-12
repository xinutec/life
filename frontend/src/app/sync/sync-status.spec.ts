import { describe, expect, it } from 'vitest';

import { SyncStatus } from './sync-status';

/** Flip the whole-app online state the way the browser does — SyncStatus keys
 *  off the window online/offline events, not a settable navigator.onLine. */
function goOffline() {
  window.dispatchEvent(new Event('offline'));
}
function goOnline() {
  window.dispatchEvent(new Event('online'));
}

describe('SyncStatus — persistent sync-health signal', () => {
  it('is synced when online with no reported errors', () => {
    goOnline();
    const s = new SyncStatus();
    expect(s.health()).toBe('synced');
    expect(s.message()).toBe('All changes synced.');
  });

  it('goes to error, surfacing the reported message, and recovers on clear', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportError('todo sync', 'Server unreachable.');
    expect(s.health()).toBe('error');
    expect(s.message()).toBe('Server unreachable.');
    s.clearError('todo sync');
    expect(s.health()).toBe('synced');
  });

  it('offline outranks an error — a failed fetch while offline is not a fault', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportError('todo sync', 'Server unreachable.');
    goOffline();
    expect(s.health()).toBe('offline');
    expect(s.message()).toContain('Offline');
    // Back online with the error still standing → the error is shown again.
    goOnline();
    expect(s.health()).toBe('error');
    expect(s.message()).toBe('Server unreachable.');
  });

  it('stays unhealthy until EVERY source has cleared', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportError('todo sync', 'todo failed');
    s.reportError('shopping sync', 'shopping failed');
    s.clearError('todo sync');
    expect(s.health()).toBe('error'); // shopping still failing
    s.clearError('shopping sync');
    expect(s.health()).toBe('synced');
  });
});

/** REGRESSION — the indicator said "synced" for a day while the log was stale.
 *
 *  `health()` had two inputs, `online` and a map of errors, so it could only
 *  ever mean "nothing has failed" — and it was rendered as "All changes
 *  synced.", which claims something stronger: that this device holds what the
 *  server holds. On 2026-09-12 those diverged for about 24 hours. Replication
 *  had pulled once and stopped (#1567); nothing threw, so nothing was reported,
 *  so the shell reassured. That is worse than saying nothing, and it is why a
 *  missing calendar square was first diagnosed as a calendar bug.
 *
 *  A success now has a TIME, and an old one is a state of its own. */
describe('SyncStatus — a success that has gone stale', () => {
  const T0 = 1_800_000_000_000;

  it('stays synced while the last success is recent', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 60_000);
    expect(s.health()).toBe('synced');
  });

  it('goes stale once nothing has succeeded for long enough, and says how long', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 6 * 60_000);
    expect(s.health()).toBe('stale');
    expect(s.message()).toContain('6 minutes');
  });

  it('a fresh success clears it', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 6 * 60_000);
    expect(s.health()).toBe('stale');
    s.reportSuccess('wellbeing sync', T0 + 6 * 60_000);
    expect(s.health()).toBe('synced');
  });

  it('takes the NEWEST success across collections, not the oldest', () => {
    // Four collections replicate. One quiet store lagging is not the app being
    // stale; every store lagging is. Keyed on the oldest, a store nobody has
    // touched would put the whole app in a warning state permanently.
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('todo sync', T0);
    s.reportSuccess('wellbeing sync', T0 + 6 * 60_000);
    s.refresh(T0 + 6 * 60_000);
    expect(s.health()).toBe('synced');
  });

  it('offline and error both outrank it', () => {
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 6 * 60_000);
    s.reportError('wellbeing sync', 'Server unreachable.');
    expect(s.health()).toBe('error');
    goOffline();
    expect(s.health()).toBe('offline');
    goOnline();
  });

  it('never says zero minutes', () => {
    // Seen by rendering it: with the cadence turned down to make the state
    // reachable, the tooltip read "Nothing has synced for 0 minutes", which
    // reassures while the icon warns. Unreachable at the shipped threshold and
    // one shortening away from being reachable again.
    goOnline();
    const s = new SyncStatus();
    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 6 * 60_000 + 1);
    expect(s.message()).toContain('6 minutes');
    expect(s.message()).not.toContain('0 minutes');
  });

  it('is not stale before anything has ever succeeded', () => {
    // A tab that has not replicated yet is not a tab showing old data, and a
    // warning in the first seconds after boot would train the eye to ignore it.
    goOnline();
    const s = new SyncStatus();
    s.refresh(T0 + 60 * 60_000);
    expect(s.health()).toBe('synced');
  });
});

/** The icon was a ternary in the template, so `stale` inherited the error glyph
 *  the day it was added — nobody chose it (#1571). A `Record` over `SyncHealth`
 *  will not compile until a new state names its own, and these assertions say
 *  which is which, so a change to any of them is deliberate. */
describe('SyncStatus — every state names its own glyph', () => {
  const T0 = 1_800_000_000_000;

  it('synced, offline, error and stale are four different icons', () => {
    goOnline();
    const s = new SyncStatus();
    expect(s.icon()).toBe('cloud_done');

    s.reportSuccess('wellbeing sync', T0);
    s.refresh(T0 + 6 * 60_000);
    expect(s.health()).toBe('stale');
    // A weaker claim than an error — "this may be old", not "something broke".
    expect(s.icon()).toBe('history');

    s.reportError('wellbeing sync', 'Server unreachable.');
    expect(s.icon()).toBe('sync_problem');

    goOffline();
    expect(s.icon()).toBe('cloud_off');
    goOnline();
  });
});
