import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { expiryInfo, monthEnd, toMonth } from './expiry';

/**
 * ⚠ The zone is pinned: date-only strings and the local clock disagree only in
 * the hours when the local and UTC days differ, so a UTC runner would assert
 * nothing. `stubEnv` because `process.env.TZ` trips opposing lint rules.
 */
beforeAll(() => vi.stubEnv('TZ', 'Europe/London'));
afterAll(() => vi.unstubAllEnvs());

// Fixed "today" for determinism.
const TODAY = new Date('2026-07-02T10:30:00Z');

describe('expiryInfo', () => {
  it('flags expired items with how long ago', () => {
    expect(expiryInfo('2026-06-29', 'day', TODAY)).toEqual({ label: 'expired 3d ago', cls: 'expired' });
    expect(expiryInfo('2026-07-01', 'day', TODAY)).toEqual({ label: 'expired 1d ago', cls: 'expired' });
  });

  it('flags today and the next few days as urgent', () => {
    expect(expiryInfo('2026-07-02', 'day', TODAY)).toEqual({ label: 'expires today', cls: 'soon' });
    expect(expiryInfo('2026-07-03', 'day', TODAY)).toEqual({ label: 'in 1d', cls: 'soon' });
    expect(expiryInfo('2026-07-05', 'day', TODAY)).toEqual({ label: 'in 3d', cls: 'soon' });
  });

  it('counts down within two weeks, then shows the date', () => {
    expect(expiryInfo('2026-07-10', 'day', TODAY)).toEqual({ label: 'in 8d', cls: 'ok' });
    expect(expiryInfo('2026-09-12', 'day', TODAY)).toEqual({ label: '12 Sept 2026', cls: 'ok' });
  });

  it('passes malformed values through unstyled', () => {
    expect(expiryInfo('not a date', 'day', TODAY)).toEqual({ label: 'not a date', cls: 'ok' });
  });
});

/** 00:30 on 15 Aug, London — still 14 Aug in UTC. */
const AFTER_MIDNIGHT = new Date('2026-08-14T23:30:00Z');

/**
 * Between midnight and 01:00 BST the UTC date is still yesterday: food that
 * expires today must not read as `in 1d`.
 */
describe('expiryInfo across midnight, local time', () => {
  it('says today for something expiring today', () => {
    expect(expiryInfo('2026-08-15', 'day', AFTER_MIDNIGHT)).toEqual({
      label: 'expires today',
      cls: 'soon',
    });
  });

  it('does not hide an item that already expired', () => {
    expect(expiryInfo('2026-08-14', 'day', AFTER_MIDNIGHT)).toEqual({
      label: 'expired 1d ago',
      cls: 'expired',
    });
  });
});

/**
 * A medicine box is printed MM/YYYY. The DATE column needs a day, so it stores
 * the month's LAST — good THROUGH June, not until the 1st. Every assertion here
 * is that the invented day never comes back out.
 */
describe('expiryInfo at month precision', () => {
  it('names the month rather than a day it does not know', () => {
    expect(expiryInfo('2028-06-30', 'month', TODAY)).toEqual({
      label: 'June 2028',
      cls: 'ok',
    });
  });

  it('warns for the whole current month, with no day countdown', () => {
    // The stored day is the 31st and "today" is the 2nd. At day precision this
    // would read "29 Jul 2026"; the box says July, and July is now.
    expect(expiryInfo('2026-07-31', 'month', TODAY)).toEqual({
      label: 'expires this month',
      cls: 'soon',
    });
  });

  it('does not turn the last days of a month into an urgent countdown', () => {
    // 2 July against a box marked August: at day precision this is 30 days out
    // and reads as a date; either way it must not be styled urgent.
    expect(expiryInfo('2026-08-31', 'month', TODAY)).toEqual({
      label: 'August 2026',
      cls: 'ok',
    });
  });

  it('says since when, not how many days ago', () => {
    expect(expiryInfo('2026-06-30', 'month', TODAY)).toEqual({
      label: 'expired since June 2026',
      cls: 'expired',
    });
  });

  it('reads the month in the reader\'s zone, not Greenwich\'s', () => {
    // 00:30 on 15 Aug London is still 14 Aug UTC: on the 1st that is a month
    // boundary, so the local month is what the comparison uses.
    expect(expiryInfo('2026-08-31', 'month', AFTER_MIDNIGHT)).toEqual({
      label: 'expires this month',
      cls: 'soon',
    });
  });
});

describe('monthEnd', () => {
  it('gives the last day, including a leap February', () => {
    expect(monthEnd('2028-06')).toBe('2028-06-30');
    expect(monthEnd('2026-02')).toBe('2026-02-28');
    expect(monthEnd('2028-02')).toBe('2028-02-29');
    expect(monthEnd('2026-12')).toBe('2026-12-31');
  });

  it('refuses anything that is not a month', () => {
    expect(monthEnd('')).toBeNull();
    expect(monthEnd('2026-13')).toBeNull();
    expect(monthEnd('2026-06-30')).toBeNull();
  });
});

describe('toMonth', () => {
  it('narrows a stored expiry to what a month input shows', () => {
    expect(toMonth('2028-06-30')).toBe('2028-06');
    expect(toMonth(null)).toBeNull();
    expect(toMonth('not a date')).toBeNull();
  });
});
