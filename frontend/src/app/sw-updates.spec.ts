import { TestBed } from '@angular/core/testing';
import { SwUpdate, UnrecoverableStateEvent, VersionEvent } from '@angular/service-worker';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SwUpdates } from './sw-updates';

function setup(isEnabled: boolean) {
  const versionUpdates = new Subject<VersionEvent>();
  const unrecoverable = new Subject<UnrecoverableStateEvent>();
  const checkForUpdate = vi.fn().mockResolvedValue(false);
  const activateUpdate = vi.fn().mockResolvedValue(true);
  TestBed.configureTestingModule({
    providers: [
      SwUpdates,
      {
        provide: SwUpdate,
        useValue: { isEnabled, versionUpdates, unrecoverable, checkForUpdate, activateUpdate },
      },
    ],
  });
  const svc = TestBed.inject(SwUpdates);
  // Only the navigation is stubbed; the policy underneath runs for real.
  const reload = vi.spyOn(svc, 'reload').mockImplementation(() => {});
  return { svc, versionUpdates, unrecoverable, checkForUpdate, activateUpdate, reload };
}

const ready = { type: 'VERSION_READY' } as VersionEvent;
const wedged = { type: 'UNRECOVERABLE_STATE', reason: 'cache is gone' } as UnrecoverableStateEvent;

function setVisibility(state: 'visible' | 'hidden') {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
  document.dispatchEvent(new Event('visibilitychange'));
}

/** Life's half only: each port adapter feeds the policy what it claims to. The
 *  policy itself (when to defer, what a manual check does) is tested in
 *  `@xinutec/ui-harness/sw-updates`, against its own code. */
describe('SwUpdates', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    sessionStorage.clear(); // the one-shot unrecoverable-recovery marker lives here
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
  });
  afterEach(() => vi.useRealTimers());

  it('checks at startup and reloads when a new version is ready right away', () => {
    const { svc, versionUpdates, checkForUpdate, activateUpdate } = setup(true);
    svc.start();
    expect(checkForUpdate).toHaveBeenCalledOnce();
    versionUpdates.next(ready);
    expect(activateUpdate).toHaveBeenCalledOnce();
  });

  it('does nothing when the service worker is disabled (dev build)', () => {
    const { svc, versionUpdates, checkForUpdate, activateUpdate } = setup(false);
    svc.start();
    expect(checkForUpdate).not.toHaveBeenCalled();
    versionUpdates.next(ready);
    expect(activateUpdate).not.toHaveBeenCalled();
  });

  it('ignores version events other than VERSION_READY', () => {
    const { svc, versionUpdates, activateUpdate } = setup(true);
    svc.start();
    versionUpdates.next({ type: 'VERSION_DETECTED' } as VersionEvent);
    versionUpdates.next({ type: 'NO_NEW_VERSION_DETECTED' } as VersionEvent);
    expect(activateUpdate).not.toHaveBeenCalled();
  });

  it('re-checks for updates when the app becomes visible again (stale tab)', () => {
    const { svc, checkForUpdate } = setup(true);
    svc.start();
    expect(checkForUpdate).toHaveBeenCalledTimes(1);
    setVisibility('hidden');
    expect(checkForUpdate).toHaveBeenCalledTimes(1); // hiding does not check
    setVisibility('visible');
    expect(checkForUpdate).toHaveBeenCalledTimes(2);
  });

  it('applies a mid-session update immediately when the app is hidden', () => {
    const { svc, versionUpdates, activateUpdate } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000);
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    versionUpdates.next(ready);
    expect(activateUpdate).toHaveBeenCalledOnce();
  });

  it('checkNow reports current when no update was found', async () => {
    const { svc } = setup(true);
    svc.start();
    await expect(svc.checkNow()).resolves.toBe('current');
  });

  it('reloads out of an unrecoverable service worker state, exactly once per tab', () => {
    const { svc, unrecoverable, reload } = setup(true);
    svc.start();
    unrecoverable.next(wedged);
    expect(reload).toHaveBeenCalledOnce();
    unrecoverable.next(wedged);
    expect(reload).toHaveBeenCalledOnce(); // one attempt, so a broken build can't loop
  });
});
