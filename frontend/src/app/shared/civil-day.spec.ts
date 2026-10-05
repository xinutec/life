import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { daysBetween, daysUntil, localDay, utcDay } from './civil-day';

/** Pinned, not inherited: the case worth testing is the hour where the local
 *  day and the UTC one differ, and a runner in UTC has no such hour. */
beforeAll(() => vi.stubEnv('TZ', 'Europe/London'));
afterAll(() => vi.unstubAllEnvs());

describe('civil days', () => {
  // 00:30 BST on 2 July is still 1 July in UTC.
  const justAfterMidnight = new Date('2026-07-01T23:30:00Z');

  it('takes today from the reader, not from Greenwich', () => {
    expect(localDay(justAfterMidnight)).toBe('2026-07-02');
    expect(daysUntil('2026-07-02', justAfterMidnight)).toBe(0);
  });

  it('reads an instant that stands for a date by its UTC day', () => {
    expect(utcDay(new Date('2026-07-01T12:00:00Z'))).toBe('2026-07-01');
  });

  it('counts across a clock change in whole days', () => {
    expect(daysBetween('2026-10-24', '2026-10-26')).toBe(2);
  });

  it('says null for something that is not a date', () => {
    expect(daysBetween('2026-07-01', 'soon')).toBeNull();
  });
});
