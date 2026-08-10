import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse } from '@angular/common/http';
import { Observable, of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import { ConnectionStatus } from '../../models';

import { BUILD_INFO } from '../../build-info';
import { Feedback } from '../../shared/feedback';
import { WellbeingReminder } from '../../shared/wellbeing-reminder';
import { SwUpdates, UpdateOutcome } from '../../sw-updates';
import { Settings } from './settings';

interface MountOpts {
  checkNow?: () => Promise<UpdateOutcome>;
  reminderAvailable?: boolean;
  /** What `/api/nextcloud/connect/status` answers. `'fail'` = it couldn't be
   *  reached, which the card must not report as "not connected". */
  ncStatus?: ConnectionStatus | 'fail';
  ncConnect?: () => Observable<{ login_url: string }>;
}

async function mount(opts: MountOpts = {}) {
  const checkNow = opts.checkNow ?? vi.fn(() => Promise.resolve('current' as const));
  const setConfig = vi.fn();
  const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
  // A plain stub for WellbeingReminder so the settings screen doesn't pull in the
  // real RxDB-backed wellbeing store.
  const reminder = {
    getConfig: () => ({ rules: [] }),
    available: opts.reminderAvailable ?? true,
    setConfig,
  };
  const status = opts.ncStatus ?? 'not_linked';
  const api = {
    nextcloudStatus: vi.fn(() =>
      status === 'fail'
        ? throwError(() => new HttpErrorResponse({ status: 0 }))
        : of({ status }),
    ),
    nextcloudConnect:
      opts.ncConnect ?? vi.fn(() => of({ login_url: 'https://nc.example/login/v2/flow/abc' })),
  };
  TestBed.configureTestingModule({
    imports: [Settings],
    providers: [
      { provide: SwUpdates, useValue: { checkNow } },
      { provide: WellbeingReminder, useValue: reminder },
      { provide: Feedback, useValue: feedback },
      { provide: LifeApi, useValue: api },
    ],
  });
  const fixture = TestBed.createComponent(Settings);
  fixture.autoDetectChanges();
  await fixture.whenStable();
  return { fixture, checkNow, setConfig, feedback, api };
}

describe('Settings', () => {
  it('shows the stamped build version', async () => {
    const { fixture } = await mount();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Version');
    expect(text).toContain(BUILD_INFO.sha);
  });

  it('checks for updates via the service worker when the button is clicked', async () => {
    const { fixture, checkNow } = await mount();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button');
    button!.click();
    await fixture.whenStable();
    expect(checkNow).toHaveBeenCalledOnce();
  });

  it('says so when the update check fails, rather than going quiet', async () => {
    const { fixture, feedback } = await mount({
      checkNow: () => Promise.resolve('failed' as const),
    });
    const button = (fixture.nativeElement as HTMLElement).querySelector('button');
    button!.click();
    await fixture.whenStable();
    expect(feedback.error).toHaveBeenCalledWith('Couldn’t update — try again.');
  });

  it('shows the wellbeing reminders section', async () => {
    const { fixture } = await mount();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Wellbeing reminders');
  });

  it('adds a reminder rule and saves it', async () => {
    const { fixture, setConfig } = await mount();
    const add = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Add reminder'))!;
    add.click();
    await fixture.whenStable();
    expect(setConfig).toHaveBeenCalledWith({
      rules: [expect.objectContaining({ time: '09:00', quietHours: 3 })],
    });
  });

  it('notes that reminders fire in the Android app when the bridge is absent', async () => {
    const { fixture } = await mount({ reminderAvailable: false });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Life Android app');
  });
  describe('the Nextcloud calendar link', () => {
    it('says it is connected, and offers no button when it is', async () => {
      // Re-linking replaces a working app password. There is nothing to fix, so
      // there is nothing to press.
      const { fixture } = await mount({ ncStatus: 'active' });
      const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
      expect(text).toContain('Connected');
      expect(connectButton(fixture)).toBeNull();
    });

    it('offers to connect when it never has been', async () => {
      const { fixture } = await mount({ ncStatus: 'not_linked' });
      expect(connectButton(fixture)).not.toBeNull();
      expect((fixture.nativeElement as HTMLElement).textContent).toContain(
        'separate from signing in',
      );
    });

    it('offers to connect again when the password stopped working', async () => {
      const { fixture } = await mount({ ncStatus: 'needs_reauth' });
      expect(connectButton(fixture)).not.toBeNull();
    });

    it('an unreachable server is unknown, not "not connected"', async () => {
      // Both guesses are actionable and wrong: "connected" hides a broken link,
      // and "not connected" invites a re-link that replaces a working password.
      const { fixture } = await mount({ ncStatus: 'fail' });
      const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
      expect(text).toContain('Couldn’t check');
      expect(connectButton(fixture)).toBeNull();
    });

    it('keeps the approval URL tappable in case the popup was blocked', async () => {
      // A WebView can refuse `window.open` even from a click. Losing the URL
      // would strand the flow with no way back to it.
      vi.spyOn(window, 'open').mockReturnValue(null);
      const { fixture } = await mount({ ncStatus: 'not_linked' });
      connectButton(fixture)?.click();
      await fixture.whenStable();
      const link = (fixture.nativeElement as HTMLElement).querySelector('a[target="_blank"]');
      expect(link?.getAttribute('href')).toBe('https://nc.example/login/v2/flow/abc');
    });

    it('says so when Nextcloud cannot be reached to start the flow', async () => {
      const { fixture, feedback } = await mount({
        ncStatus: 'not_linked',
        ncConnect: () => throwError(() => new HttpErrorResponse({ status: 0 })),
      });
      connectButton(fixture)?.click();
      await fixture.whenStable();
      expect(feedback.error).toHaveBeenCalledWith(expect.stringContaining('Nextcloud'));
    });
  });
});

function connectButton(fixture: { nativeElement: unknown }): HTMLButtonElement | null {
  const buttons = [
    ...(fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ] as HTMLButtonElement[];
  return buttons.find((b) => (b.textContent ?? '').includes('Connect')) ?? null;
}
