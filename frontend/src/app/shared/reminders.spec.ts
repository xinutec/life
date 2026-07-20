import { afterEach, describe, expect, it, vi } from 'vitest';

import { Reminders } from './reminders';

// The native bridge lives on window; fake it per-test.
interface TestWin {
  ReminderBridge?: unknown;
}
const w = window as unknown as TestWin;

describe('Reminders bridge service', () => {
  afterEach(() => {
    delete w.ReminderBridge;
  });

  it('available is false in a plain browser', () => {
    expect(new Reminders().available).toBe(false);
  });

  it('schedule and cancel are safe no-ops without the bridge', () => {
    const r = new Reminders();
    expect(() => r.schedule('id', 1, 't', 'b', '/x')).not.toThrow();
    expect(() => r.cancel('id')).not.toThrow();
  });

  it('forwards schedule/cancel to the native bridge when present', () => {
    const schedule = vi.fn();
    const cancel = vi.fn();
    w.ReminderBridge = { available: () => true, schedule, cancel };
    const r = new Reminders();
    expect(r.available).toBe(true);
    r.schedule('wellbeing-daily', 123, 'T', 'B', '/today');
    expect(schedule).toHaveBeenCalledWith('wellbeing-daily', 123, 'T', 'B', '/today');
    r.cancel('wellbeing-daily');
    expect(cancel).toHaveBeenCalledWith('wellbeing-daily');
  });
});
