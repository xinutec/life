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
  // Only the navigation is stubbed, so applyUpdate() runs for real — including
  // its failure path, which is where the interesting behaviour lives.
  const reload = vi.spyOn(svc, 'reload').mockImplementation(() => {});
  const apply = vi.spyOn(svc, 'applyUpdate');
  return { svc, versionUpdates, unrecoverable, checkForUpdate, activateUpdate, apply, reload };
}

const ready = { type: 'VERSION_READY' } as VersionEvent;
const wedged = { type: 'UNRECOVERABLE_STATE', reason: 'cache is gone' } as UnrecoverableStateEvent;

function setVisibility(state: 'visible' | 'hidden') {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
  document.dispatchEvent(new Event('visibilitychange'));
}

describe('SwUpdates', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    sessionStorage.clear(); // the one-shot unrecoverable-recovery marker lives here
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
  });
  afterEach(() => vi.useRealTimers());

  it('checks at startup and reloads when a new version is ready right away', () => {
    const { svc, versionUpdates, checkForUpdate, apply } = setup(true);
    svc.start();
    expect(checkForUpdate).toHaveBeenCalledOnce();
    versionUpdates.next(ready);
    expect(apply).toHaveBeenCalledOnce();
  });

  it('does nothing when the service worker is disabled (dev build)', () => {
    const { svc, versionUpdates, checkForUpdate, apply } = setup(false);
    svc.start();
    expect(checkForUpdate).not.toHaveBeenCalled();
    versionUpdates.next(ready);
    expect(apply).not.toHaveBeenCalled();
  });

  it('ignores version events other than VERSION_READY', () => {
    const { svc, versionUpdates, apply } = setup(true);
    svc.start();
    versionUpdates.next({ type: 'VERSION_DETECTED' } as VersionEvent);
    versionUpdates.next({ type: 'NO_NEW_VERSION_DETECTED' } as VersionEvent);
    expect(apply).not.toHaveBeenCalled();
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

  it('defers a mid-session update to the next backgrounding, not mid-use', () => {
    const { svc, versionUpdates, apply } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000); // long past the startup window
    versionUpdates.next(ready);
    expect(apply).not.toHaveBeenCalled(); // user may be mid-edit
    setVisibility('hidden');
    expect(apply).toHaveBeenCalledOnce(); // reloads invisibly once backgrounded
  });

  it('applies a mid-session update immediately when the app is hidden', () => {
    const { svc, versionUpdates, apply } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000);
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    versionUpdates.next(ready);
    expect(apply).toHaveBeenCalledOnce();
  });

  it('checkNow applies immediately — the user explicitly asked', async () => {
    const { svc, versionUpdates, checkForUpdate, apply } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000);
    checkForUpdate.mockResolvedValueOnce(true);
    await expect(svc.checkNow()).resolves.toBe('updating');
    versionUpdates.next(ready);
    expect(apply).toHaveBeenCalledOnce(); // no deferral on a manual check
  });

  it('checkNow reports current when no update was found', async () => {
    const { svc } = setup(true);
    svc.start();
    await expect(svc.checkNow()).resolves.toBe('current');
  });

  it('checkNow activates an ALREADY-staged update instead of reporting current', async () => {
    // The stale-phone bug: an update downloaded mid-session was deferred, then a
    // later checkForUpdate() sees nothing newer and returns false — so the manual
    // check must apply the pending build rather than say "you're on the latest".
    const { svc, versionUpdates, checkForUpdate, apply } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000); // mid-session, visible → the update defers
    versionUpdates.next(ready);
    expect(apply).not.toHaveBeenCalled(); // held, not applied
    checkForUpdate.mockResolvedValue(false); // nothing newer than the staged build
    await expect(svc.checkNow()).resolves.toBe('updating');
    expect(apply).toHaveBeenCalledOnce();
  });

  it('a failed manual check leaves the mid-session deferral armed', async () => {
    // checkForUpdate() rejects on any error — offline being the everyday one. If
    // that left "the user asked" latched on, the next background update would
    // reload straight through a half-typed form.
    const { svc, versionUpdates, checkForUpdate, apply } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000);
    checkForUpdate.mockRejectedValueOnce(new Error('offline'));
    await expect(svc.checkNow()).resolves.toBe('failed');
    versionUpdates.next(ready);
    expect(apply).not.toHaveBeenCalled(); // still deferring, as if nothing was asked
    setVisibility('hidden');
    expect(apply).toHaveBeenCalledOnce();
  });

  it('reports failure when a staged update cannot be activated, and keeps it staged', async () => {
    const { svc, versionUpdates, activateUpdate, reload } = setup(true);
    svc.start();
    vi.advanceTimersByTime(60_000);
    versionUpdates.next(ready); // staged, held
    activateUpdate.mockRejectedValueOnce(new Error('worker gone'));
    await expect(svc.checkNow()).resolves.toBe('failed');
    expect(reload).not.toHaveBeenCalled(); // never claim an update that didn't happen
    // Still staged rather than forgotten, so the next attempt picks it up.
    await expect(svc.checkNow()).resolves.toBe('updating');
    expect(reload).toHaveBeenCalledOnce();
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
