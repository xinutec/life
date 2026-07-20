import { describe, expect, it } from 'vitest';

import { createRule, nextFireForRule, parseHhMm } from './wellbeing-reminder';

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
