import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { warrantyInfo } from './warranty';

/** The zone is pinned, not inherited — see the note in `expiry.spec.ts`. Every
 *  case here is arithmetic between a stored instant and a reader's clock, and
 *  the two disagree only in the hours where the local day and the UTC one are
 *  different days. */
beforeAll(() => vi.stubEnv('TZ', 'Europe/London'));
afterAll(() => vi.unstubAllEnvs());

const TODAY = new Date('2026-07-02T10:30:00Z');
const at = (iso: string) => `${iso}T12:00:00Z`;

describe('warrantyInfo', () => {
  it('says nothing at all when no warranty was recorded', () => {
    // The common case, and the one that must not render. Most purchases have no
    // warranty; "no cover" and "nobody wrote it down" are different claims and
    // only one of them is ours to make.
    expect(warrantyInfo(null, TODAY)).toBeNull();
    expect(warrantyInfo('not a date', TODAY)).toBeNull();
  });

  it('names the end date while it is comfortably away', () => {
    expect(warrantyInfo(at('2027-03-03'), TODAY)).toEqual({
      label: 'under warranty until 3 Mar 2027',
      cls: 'ok',
    });
  });

  it('counts down once a decision is worth making', () => {
    // Inside three months the useful fact is how long is left, not the date:
    // this is when claiming it, extending it or replacing the thing is a choice
    // rather than something to defer.
    expect(warrantyInfo(at('2026-08-01'), TODAY)).toEqual({
      label: 'under warranty for another 30 days',
      cls: 'soon',
    });
    expect(warrantyInfo(at('2026-07-02'), TODAY)).toEqual({
      label: 'warranty ends today',
      cls: 'soon',
    });
    // Singular, because "another 1 days" is how you can tell nobody read it.
    expect(warrantyInfo(at('2026-07-03'), TODAY)).toEqual({
      label: 'under warranty for another 1 day',
      cls: 'soon',
    });
  });

  it('says when it ended, rather than how long ago', () => {
    // The date is what a shop asks for. "Eight months ago" is not something you
    // can put on a claim form.
    expect(warrantyInfo(at('2026-06-30'), TODAY)).toEqual({
      label: 'warranty ended 30 Jun 2026',
      cls: 'expired',
    });
  });

  it("reads the day in the reader's zone, not Greenwich's", () => {
    // 00:30 on 15 Aug in London is still 14 Aug in UTC — the hour that produced
    // a real off-by-a-day in the expiry code twice.
    const afterMidnight = new Date('2026-08-14T23:30:00Z');
    expect(warrantyInfo(at('2026-08-15'), afterMidnight)).toEqual({
      label: 'warranty ends today',
      cls: 'soon',
    });
    expect(warrantyInfo(at('2026-08-14'), afterMidnight)).toEqual({
      label: 'warranty ended 14 Aug 2026',
      cls: 'expired',
    });
  });
});
