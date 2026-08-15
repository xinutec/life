import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { BinDay } from './models';
import { nextCollections, shortKind } from './bins';

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

/** 10 Aug 2026, mid-morning UTC. */
const NOW = new Date('2026-08-10T09:00:00Z');

const day = (date: string, kind: string): BinDay => ({ date, kind });

describe('shortKind', () => {
  it('drops the council’s " collection" suffix', () => {
    expect(shortKind('Food waste collection')).toBe('Food waste');
    expect(shortKind('Paper and cardboard (blue sacks) collection')).toBe(
      'Paper and cardboard (blue sacks)',
    );
  });

  it('leaves a name that does not end that way alone', () => {
    // Only the exact suffix goes. Trimming on a guess would mangle a name the
    // council words differently.
    expect(shortKind('Garden waste')).toBe('Garden waste');
    expect(shortKind('Collection of bulky waste')).toBe('Collection of bulky waste');
  });
});

describe('nextCollections', () => {
  it('puts one morning on one row', () => {
    // Three bins really do go out together, and three rows saying the same date
    // is a list you have to read to learn one thing.
    const rows = nextCollections(
      [
        day('2026-08-13', 'Food waste collection'),
        day('2026-08-13', 'Rubbish collection'),
        day('2026-08-13', 'Paper and cardboard (blue sacks) collection'),
      ],
      NOW,
    );

    expect(rows).toHaveLength(1);
    expect(rows[0].kinds).toEqual([
      'Food waste',
      'Rubbish',
      'Paper and cardboard (blue sacks)',
    ]);
  });

  it('orders mornings soonest first', () => {
    const rows = nextCollections(
      [
        day('2026-08-27', 'Rubbish collection'),
        day('2026-08-13', 'Food waste collection'),
        day('2026-08-20', 'Recycling collection'),
      ],
      NOW,
    );

    expect(rows.map((r) => r.date)).toEqual(['2026-08-13', '2026-08-20', '2026-08-27']);
  });

  it('counts today and tomorrow as imminent, and nothing else', () => {
    // The bins are a thing you act on the night before, so a "soon" that
    // reached into next week would make the emphasis mean nothing.
    const rows = nextCollections(
      [
        day('2026-08-10', 'Rubbish collection'),
        day('2026-08-11', 'Food waste collection'),
        day('2026-08-12', 'Recycling collection'),
      ],
      NOW,
    );

    expect(rows.map((r) => [r.when, r.imminent])).toEqual([
      ['today', true],
      ['tomorrow', true],
      ['in 2 days', false],
    ]);
  });

  it('writes a far-off morning as a date, not a countdown', () => {
    // "in 43 days" is not a date anybody can place. September deliberately:
    // en-GB abbreviates it "Sept", the one month that is not three letters,
    // so this also pins that we are rendering in the locale we think we are.
    const rows = nextCollections([day('2026-09-24', 'Rubbish collection')], NOW);
    expect(rows[0].when).toBe('Thu 24 Sept');
    expect(rows[0].imminent).toBe(false);
  });

  it('names the day the council stated, whatever the hour it is read at', () => {
    // The feed states a bare day with no timezone, and it must appear as that
    // day — never the one before, however late the reader is up.
    //
    // ⚠ **This used to assert `tomorrow` for a collection on the very day the
    // reader was already in**, from a `now` of 23:30Z — which is 00:30 the next
    // morning in London. The comment was about how the DATE is parsed, but the
    // assertion pinned the reference day, and pinned it wrong: the bins were
    // going out that morning. It also only meant that in a zone east of UTC, so
    // on a machine in UTC it quietly tested nothing. The parsing it was actually
    // guarding is checked here, and the reference day below.
    const lateAtNight = new Date('2026-08-10T23:30:00Z');
    const rows = nextCollections([day('2026-09-24', 'Rubbish collection')], lateAtNight);
    expect(rows[0].when).toBe('Thu 24 Sept');
  });

  it('no feed is no rows, not a broken card', () => {
    expect(nextCollections([], NOW)).toEqual([]);
  });
});

/** 00:30 on 15 Aug, London — still 14 Aug by the clock in Greenwich. */
const AFTER_MIDNIGHT = new Date('2026-08-14T23:30:00Z');

/**
 * The hour when the local day and the UTC day are different days.
 *
 * ⚠ **Every other case in this file is set mid-morning UTC, where the two agree
 * — which is why none of them could see this.** `daysUntil` took its idea of
 * "today" from the UTC fields, so between midnight and 01:00 BST the app was a
 * whole day behind: a collection happening THAT MORNING read as "tomorrow",
 * which is how a bin gets missed, and the next morning's read as "in 2 days".
 */
describe('nextCollections across midnight, local time', () => {
  it('calls this morning’s collection today, not tomorrow', () => {
    const [row] = nextCollections([day('2026-08-15', 'Food waste collection')], AFTER_MIDNIGHT);
    expect(row.when).toBe('today');
    expect(row.imminent).toBe(true);
  });

  it('calls the next morning’s collection tomorrow, not in 2 days', () => {
    const [row] = nextCollections([day('2026-08-16', 'Rubbish collection')], AFTER_MIDNIGHT);
    expect(row.when).toBe('tomorrow');
    expect(row.imminent).toBe(true);
  });

  it('still counts the far ones from the same day', () => {
    const [row] = nextCollections([day('2026-08-18', 'Recycling collection')], AFTER_MIDNIGHT);
    expect(row.when).toBe('in 3 days');
    expect(row.imminent).toBe(false);
  });
});
