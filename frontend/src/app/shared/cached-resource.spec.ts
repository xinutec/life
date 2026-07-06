import { Observable, Subject, of, throwError } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CachedResource } from './cached-resource';

describe('CachedResource — retain-across-tabs cached read model', () => {
  it('starts empty and unloaded', () => {
    const r = new CachedResource<number[]>(() => of([]));
    expect(r.value()).toBeNull();
    expect(r.loaded()).toBe(false);
    expect(r.error()).toBe(false);
    expect(r.refreshing()).toBe(false);
  });

  it('populates on refresh: value set, loaded true, no error, not refreshing', () => {
    const r = new CachedResource<number[]>(() => of([1, 2, 3]));
    r.refresh();
    expect(r.value()).toEqual([1, 2, 3]);
    expect(r.loaded()).toBe(true);
    expect(r.error()).toBe(false);
    expect(r.refreshing()).toBe(false);
  });

  it('surfaces an error only when the FIRST load fails (nothing cached)', () => {
    const r = new CachedResource<number[]>(() => throwError(() => new Error('offline')));
    r.refresh();
    expect(r.loaded()).toBe(true);
    expect(r.error()).toBe(true);
    expect(r.value()).toBeNull();
  });

  it('keeps the cached value and does NOT error when a background refresh fails', () => {
    // The core retain behaviour: an offline revisit shows stale data, not a retry.
    let source: Observable<number[]> = of([1, 2, 3]);
    const r = new CachedResource<number[]>(() => source);
    r.refresh(); // first load succeeds
    source = throwError(() => new Error('offline'));
    r.refresh(); // background refresh fails
    expect(r.value()).toEqual([1, 2, 3]); // retained
    expect(r.error()).toBe(false); // not an error — we still have data
    expect(r.loaded()).toBe(true);
  });

  it('reports refreshing while a fetch is in flight, then clears it', () => {
    const gate = new Subject<number[]>();
    const r = new CachedResource<number[]>(() => gate);
    r.refresh();
    expect(r.refreshing()).toBe(true);
    expect(r.value()).toBeNull(); // nothing shown yet on the very first load
    gate.next([9]);
    gate.complete();
    expect(r.refreshing()).toBe(false);
    expect(r.value()).toEqual([9]);
  });

  it('cancels an in-flight fetch when refresh() fires again (switchMap)', () => {
    const first = new Subject<number[]>();
    const second = new Subject<number[]>();
    let call = 0;
    const r = new CachedResource<number[]>(() => (call++ === 0 ? first : second));
    r.refresh(); // subscribes to `first`
    r.refresh(); // switches to `second`, unsubscribing `first`
    first.next([1]); // stale — must be ignored
    second.next([2]);
    expect(r.value()).toEqual([2]);
  });
});
