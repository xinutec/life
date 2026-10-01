import { TestBed } from '@angular/core/testing';
import { BehaviorSubject, NEVER, Observable } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WellbeingStore } from '../sync/wellbeing-store';
import { WellbeingReminder, createRule, nextFireForRule, parseHhMm } from './wellbeing-reminder';

describe('parseHhMm', () => {
  it('parses valid local times', () => {
    expect(parseHhMm('09:00')).toEqual([9, 0]);
    expect(parseHhMm('23:59')).toEqual([23, 59]);
    expect(parseHhMm('0:05')).toEqual([0, 5]);
  });

  it('rejects malformed times', () => {
    expect(parseHhMm('9')).toBeNull();
    expect(parseHhMm('24:00')).toBeNull();
    expect(parseHhMm('09:60')).toBeNull();
    expect(parseHhMm('ab:cd')).toBeNull();
    expect(parseHhMm('x09:00')).toBeNull();
    expect(parseHhMm('09:00x')).toBeNull();
    expect(parseHhMm('')).toBeNull();
  });
});

describe('createRule', () => {
  it('mints a fresh id and sensible defaults', () => {
    const a = createRule();
    const b = createRule();
    expect(a.time).toBe('09:00');
    expect(a.quietHours).toBe(3);
    expect(a.id).not.toBe(b.id);
  });

  it('honours explicit time and window', () => {
    expect(createRule('18:00', 6)).toMatchObject({ time: '18:00', quietHours: 6 });
  });
});

describe('nextFireForRule', () => {
  const rule = { id: 'r', time: '09:00', quietHours: 3 };

  it('returns null for a malformed time', () => {
    expect(nextFireForRule({ ...rule, time: 'nope' }, new Date(2026, 6, 20, 8), null)).toBeNull();
  });

  it('fires at the next future occurrence when there are no check-ins', () => {
    const at = nextFireForRule(rule, new Date(2026, 6, 20, 8, 0), null);
    expect(new Date(at!)).toEqual(new Date(2026, 6, 20, 9, 0, 0, 0));
  });

  it('rolls to tomorrow once the time has passed today', () => {
    const at = nextFireForRule(rule, new Date(2026, 6, 20, 10, 0), null);
    expect(new Date(at!)).toEqual(new Date(2026, 6, 21, 9, 0, 0, 0));
  });

  it('fires today when the quiet window has elapsed by the reminder time', () => {
    // Last check-in 5am; at 9am the gap is 4h ≥ 3h.
    const last = new Date(2026, 6, 20, 5, 0).getTime();
    const at = nextFireForRule(rule, new Date(2026, 6, 20, 8, 0), last);
    expect(new Date(at!)).toEqual(new Date(2026, 6, 20, 9, 0, 0, 0));
  });

  it('skips to tomorrow when a recent check-in leaves the window unmet', () => {
    // Last check-in 7am; at 9am the gap is only 2h < 3h, so today is suppressed.
    const last = new Date(2026, 6, 20, 7, 0).getTime();
    const at = nextFireForRule(rule, new Date(2026, 6, 20, 8, 0), last);
    expect(new Date(at!)).toEqual(new Date(2026, 6, 21, 9, 0, 0, 0));
  });

  it('treats an evening rule with a longer window independently', () => {
    // 6pm / 6h: last check-in noon → gap 6h ≥ 6h → fires today at 18:00.
    const evening = { id: 'e', time: '18:00', quietHours: 6 };
    const last = new Date(2026, 6, 20, 12, 0).getTime();
    const at = nextFireForRule(evening, new Date(2026, 6, 20, 13, 0), last);
    expect(new Date(at!)).toEqual(new Date(2026, 6, 20, 18, 0, 0, 0));
  });
});

/** The service against the phone's reminder port: what it schedules and cancels. */
describe('WellbeingReminder', () => {
  interface Sent {
    op: 'schedule' | 'cancel';
    id: string;
    whenMs?: number;
    url?: string;
  }
  let sent: Sent[];
  let checkins: BehaviorSubject<{ recordedAt: string }[]>;

  beforeEach(() => {
    localStorage.clear();
    sent = [];
    checkins = new BehaviorSubject<{ recordedAt: string }[]>([]);
    Object.assign(window, {
      ReminderBridge: { postMessage: (m: string) => sent.push(JSON.parse(m) as Sent) },
    });
  });

  afterEach(() => {
    delete (window as { ReminderBridge?: unknown }).ReminderBridge;
  });

  /** A fresh service, as on an app open: config and armed ids come from storage.
   *  `items$` is the check-ins, or NEVER for an app closed before they loaded. */
  function open(items$: Observable<{ recordedAt: string }[]> = checkins): WellbeingReminder {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      providers: [{ provide: WellbeingStore, useValue: { items$ } }],
    });
    const reminder = TestBed.inject(WellbeingReminder);
    reminder.init();
    return reminder;
  }

  const rule = (id: string, time = '09:00') => ({ id, time, quietHours: 3 });

  it('arms one alarm per rule, opening Today when tapped', () => {
    open().setConfig({ rules: [rule('a'), rule('b', '20:00')] });
    const scheduled = sent.filter((m) => m.op === 'schedule');
    expect(scheduled.map((m) => m.id)).toEqual(['a', 'b']);
    expect(scheduled.every((m) => m.url === '/today')).toBe(true);
  });

  it('cancels the alarm of a removed rule', () => {
    const reminder = open();
    reminder.setConfig({ rules: [rule('a'), rule('b')] });
    sent = [];
    reminder.setConfig({ rules: [rule('a')] });
    expect(sent).toContainEqual({ op: 'cancel', id: 'b' });
    expect(sent.some((m) => m.op === 'schedule' && m.id === 'b')).toBe(false);
  });

  it('cancels it on the next open when the app closed before re-arming', () => {
    open().setConfig({ rules: [rule('a'), rule('b')] });
    // Removed while the check-ins had not loaded: saved, but nothing re-armed.
    open(NEVER).setConfig({ rules: [rule('a')] });
    sent = [];
    open();
    expect(sent).toContainEqual({ op: 'cancel', id: 'b' });
    expect(sent.some((m) => m.op === 'schedule' && m.id === 'a')).toBe(true);
  });

  it('cancels the alarm of a rule edited to a time it cannot fire at', () => {
    const reminder = open();
    reminder.setConfig({ rules: [rule('a')] });
    sent = [];
    reminder.setConfig({ rules: [rule('a', 'soon')] });
    expect(sent).toEqual([{ op: 'cancel', id: 'a' }]);
  });

  it('moves the alarm to tomorrow when a check-in lands inside the quiet window', () => {
    // 08:00, a 09:00 rule with three quiet hours.
    vi.useFakeTimers({ toFake: ['Date'] });
    const now = new Date(2026, 9, 1, 8, 0);
    vi.setSystemTime(now);
    open().setConfig({ rules: [rule('a')] });
    expect(sent.at(-1)?.whenMs).toBe(new Date(2026, 9, 1, 9, 0).getTime());
    // Newest first, as the store lists them; the older one must not count.
    checkins.next([
      { recordedAt: now.toISOString() },
      { recordedAt: new Date(2026, 8, 29, 8, 0).toISOString() },
    ]);
    expect(sent.at(-1)?.whenMs).toBe(new Date(2026, 9, 2, 9, 0).getTime());
    vi.useRealTimers();
  });

  it('drops stored rules it cannot fire, and survives a corrupt store', () => {
    localStorage.setItem(
      'life.reminder.wellbeing',
      JSON.stringify({
        rules: [
          rule('ok'),
          { id: 'always', time: '09:00', quietHours: 0 },
          rule('bad', '25:00'),
          { id: 'x', time: '09:00' },
          { id: 7, time: '09:00', quietHours: 3 },
          { id: 'y', time: '09:00', quietHours: -1 },
          'not a rule',
        ],
      }),
    );
    expect(
      open()
        .getConfig()
        .rules.map((r) => r.id),
    ).toEqual(['ok', 'always']);
    localStorage.setItem('life.reminder.wellbeing', '{not json');
    expect(open().getConfig().rules).toEqual([]);
  });

  it('does nothing outside the Android app', () => {
    delete (window as { ReminderBridge?: unknown }).ReminderBridge;
    const reminder = open();
    reminder.setConfig({ rules: [rule('a')] });
    expect(reminder.available).toBe(false);
    expect(sent).toEqual([]);
  });
});
