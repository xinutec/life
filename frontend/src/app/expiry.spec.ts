import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { expiryInfo } from './expiry';

/**
 * ⚠ **The zone is pinned, not inherited.** Every date case below is arithmetic
 * between a bare `YYYY-MM-DD` and a reader's clock, and the two disagree only in
 * the hours where the local calendar day and the UTC one are different days. A
 * test that reads the runner's zone passes on a laptop in London and asserts
 * nothing on a machine in UTC — which is how a real off-by-a-day survived here.
 *
 * `stubEnv` rather than assigning `process.env.TZ`: `process` is untyped in this
 * project, and the two linters want opposite spellings of the index access —
 * TS4111 demands brackets, `dot-notation` demands dots. Setting it after the
 * module's `Date`s are built is safe, because those are absolute instants; only
 * the local-field reads inside the functions under test depend on the zone.
 */
beforeAll(() => vi.stubEnv('TZ', 'Europe/London'));
afterAll(() => vi.unstubAllEnvs());

// Fixed "today" for determinism.
const TODAY = new Date('2026-07-02T10:30:00Z');

describe('expiryInfo', () => {
  it('flags expired items with how long ago', () => {
    expect(expiryInfo('2026-06-29', TODAY)).toEqual({ label: 'expired 3d ago', cls: 'expired' });
    expect(expiryInfo('2026-07-01', TODAY)).toEqual({ label: 'expired 1d ago', cls: 'expired' });
  });

  it('flags today and the next few days as urgent', () => {
    expect(expiryInfo('2026-07-02', TODAY)).toEqual({ label: 'expires today', cls: 'soon' });
    expect(expiryInfo('2026-07-03', TODAY)).toEqual({ label: 'in 1d', cls: 'soon' });
    expect(expiryInfo('2026-07-05', TODAY)).toEqual({ label: 'in 3d', cls: 'soon' });
  });

  it('counts down within two weeks, then shows the date', () => {
    expect(expiryInfo('2026-07-10', TODAY)).toEqual({ label: 'in 8d', cls: 'ok' });
    expect(expiryInfo('2026-09-12', TODAY)).toEqual({ label: '12 Sept 2026', cls: 'ok' });
  });

  it('passes malformed values through unstyled', () => {
    expect(expiryInfo('not a date', TODAY)).toEqual({ label: 'not a date', cls: 'ok' });
  });
});

/** 00:30 on 15 Aug, London — still 14 Aug in UTC. */
const AFTER_MIDNIGHT = new Date('2026-08-14T23:30:00Z');

/**
 * The same hour, and the same fault — `bins.ts` cited this function as the
 * pattern it was following, and it was following it into the bug.
 *
 * Between midnight and 01:00 BST the UTC date is still yesterday, so anything
 * keyed to `now.getUTCDate()` is a day behind. Food that expires today read as
 * `in 1d`, which is the wrong way round to be wrong about something you are
 * deciding whether to eat.
 */
describe('expiryInfo across midnight, local time', () => {
  it('says today for something expiring today', () => {
    expect(expiryInfo('2026-08-15', AFTER_MIDNIGHT)).toEqual({
      label: 'expires today',
      cls: 'soon',
    });
  });

  it('does not hide an item that already expired', () => {
    expect(expiryInfo('2026-08-14', AFTER_MIDNIGHT)).toEqual({
      label: 'expired 1d ago',
      cls: 'expired',
    });
  });
});
