import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { BUILD_INFO } from '../../build-info';
import { WellbeingReminder } from '../../shared/wellbeing-reminder';
import { SwUpdates } from '../../sw-updates';
import { Settings } from './settings';

interface MountOpts {
  checkNow?: () => Promise<'current' | 'updating' | 'unavailable'>;
  reminderAvailable?: boolean;
}

async function mount(opts: MountOpts = {}) {
  const checkNow = opts.checkNow ?? vi.fn(() => Promise.resolve('current' as const));
  const setConfig = vi.fn();
  // A plain stub for WellbeingReminder so the settings screen doesn't pull in the
  // real RxDB-backed wellbeing store.
  const reminder = {
    getConfig: () => ({ rules: [] }),
    available: opts.reminderAvailable ?? true,
    setConfig,
  };
  TestBed.configureTestingModule({
    imports: [Settings],
    providers: [
      { provide: SwUpdates, useValue: { checkNow } },
      { provide: WellbeingReminder, useValue: reminder },
    ],
  });
  const fixture = TestBed.createComponent(Settings);
  fixture.autoDetectChanges();
  await fixture.whenStable();
  return { fixture, checkNow, setConfig };
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
});
