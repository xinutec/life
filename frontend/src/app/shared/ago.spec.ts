import { describe, expect, it } from 'vitest';

import { ago } from './ago';

/** `ago` had no test, because until the clock became a parameter there was no
 *  instant to test it at — every case would have been arithmetic against
 *  whatever moment the suite ran. */
describe('ago', () => {
  const NOW = Date.UTC(2026, 8, 12, 12, 0, 0); // 2026-09-12T12:00:00Z
  const hoursBefore = (h: number) => NOW - h * 3_600_000;

  it('says today for the last 24 hours', () => {
    expect(ago(NOW, NOW)).toBe('today');
    expect(ago(hoursBefore(23), NOW)).toBe('today');
  });

  it('says yesterday, then counts days, up to a week', () => {
    expect(ago(hoursBefore(24), NOW)).toBe('yesterday');
    expect(ago(hoursBefore(48), NOW)).toBe('2 days ago');
    expect(ago(hoursBefore(24 * 6), NOW)).toBe('6 days ago');
  });

  it('becomes a date once the relative form stops placing it', () => {
    // "43 days ago" is not a date anybody can situate, which is the reason the
    // module gives for the switch. Asserted as "not the relative form" rather
    // than against a locale string, since the output is the reader's locale.
    const out = ago(hoursBefore(24 * 43), NOW);
    expect(out).not.toContain('days ago');
    expect(out).toMatch(/\d/);
  });

  it('counts elapsed 24-hour periods, NOT calendar days', () => {
    // ⚠ Pinned deliberately, and it is worth seeing. 23:00 the previous
    // calendar day reads as "today" at 09:00, because only ten hours have
    // passed. The module argues recency over precision, so this is a choice —
    // but an app that says "today" and "yesterday" is making a calendar claim,
    // and this test is where a decision to change it would start.
    const nineAm = Date.UTC(2026, 8, 12, 9, 0, 0);
    const elevenPmYesterday = Date.UTC(2026, 8, 11, 23, 0, 0);
    expect(ago(elevenPmYesterday, nineAm)).toBe('today');
  });

  it('does not go negative on a timestamp from the future', () => {
    // Clock skew between a device and the server is ordinary; "-1 days ago"
    // would not be.
    expect(ago(NOW + 3_600_000, NOW)).toBe('today');
  });
});
