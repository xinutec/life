import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { ago } from './ago';

/** Pinned, not inherited: calendar days differ by zone. */
beforeAll(() => vi.stubEnv('TZ', 'Europe/London'));
afterAll(() => vi.unstubAllEnvs());

/** `now` is a parameter, so every case states its instant. */
describe('ago', () => {
  const NOW = Date.UTC(2026, 8, 12, 12, 0, 0); // 2026-09-12T12:00:00Z
  const hoursBefore = (h: number) => NOW - h * 3_600_000;

  it('says today for anything earlier the same day', () => {
    expect(ago(NOW, NOW)).toBe('today');
    expect(ago(hoursBefore(11), NOW)).toBe('today'); // 02:00 BST
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

  it('counts calendar days, not elapsed hours', () => {
    // Ten hours apart, but last night is yesterday.
    const nineAm = Date.UTC(2026, 8, 12, 8, 0, 0); // 09:00 BST
    const elevenPmYesterday = Date.UTC(2026, 8, 11, 22, 0, 0); // 23:00 BST
    expect(ago(elevenPmYesterday, nineAm)).toBe('yesterday');
  });

  it('takes the day from the reader, not from Greenwich', () => {
    // 23:30 UTC is 00:30 BST the next day.
    const justAfterMidnight = Date.UTC(2026, 8, 11, 23, 30, 0);
    expect(ago(Date.UTC(2026, 8, 11, 22, 30, 0), justAfterMidnight)).toBe('yesterday');
  });

  it('does not go negative on a timestamp from the future', () => {
    // Clock skew between a device and the server is ordinary; "-1 days ago"
    // would not be.
    expect(ago(NOW + 3_600_000, NOW)).toBe('today');
  });
});
