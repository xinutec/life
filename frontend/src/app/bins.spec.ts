import { describe, expect, it } from 'vitest';

import { BinDay } from './models';
import { nextCollections, shortKind } from './bins';

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

  it('reads the day in UTC, as the server states it', () => {
    // The feed states a bare day with no timezone. Reading it as LOCAL midnight
    // would flip a collection to "tomorrow" for anyone west of Greenwich.
    const justBeforeMidnightUtc = new Date('2026-08-10T23:30:00Z');
    const rows = nextCollections([day('2026-08-11', 'Rubbish collection')], justBeforeMidnightUtc);
    expect(rows[0].when).toBe('tomorrow');
  });

  it('no feed is no rows, not a broken card', () => {
    expect(nextCollections([], NOW)).toEqual([]);
  });
});
