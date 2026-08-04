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
    const postMessage = vi.fn<(message: string) => void>();
    w.ReminderBridge = { postMessage };
    const r = new Reminders();
    expect(r.available).toBe(true);
    r.schedule('wellbeing-daily', 123, 'T', 'B', '/today');
    expect(JSON.parse(postMessage.mock.calls[0][0])).toEqual({
      op: 'schedule',
      id: 'wellbeing-daily',
      whenMs: 123,
      title: 'T',
      body: 'B',
      url: '/today',
    });
    r.cancel('wellbeing-daily');
    expect(JSON.parse(postMessage.mock.calls[1][0])).toEqual({
      op: 'cancel',
      id: 'wellbeing-daily',
    });
  });

  // The app is sideloaded, so the page always updates first and can be running
  // against a wrapper that still injects the old three-method shape. That has no
  // postMessage: reminders read as unavailable and stay quiet, rather than
  // throwing the first time something schedules one.
  it('an older app reads as no bridge at all', () => {
    w.ReminderBridge = { available: () => true, schedule: vi.fn(), cancel: vi.fn() };
    const r = new Reminders();
    expect(r.available).toBe(false);
    expect(() => r.schedule('id', 1, 't', 'b', '/x')).not.toThrow();
  });
});
