import { describe, expect, it } from 'vitest';

import { WellbeingDoc } from '../../sync/wellbeing-store';
import {
  CalendarDay,
  CalendarMonth,
  bandsFor,
  buildCalendar,
  localDayKey,
  tokensAcross,
} from './emotion-calendar-model';

/** ⚠ Every test that names a day passes an explicit `tz`. Without one these
 *  assert whatever zone the runner happens to be in, which passes on this
 *  machine and fails in CI — the class of green test that proves nothing. */
const LONDON = 'Europe/London';

function doc(recordedAt: string, emotions: string[], scoreTenths = 40): WellbeingDoc {
  return {
    ulid: recordedAt,
    id: null,
    recordedAt,
    scoreTenths,
    energyTenths: null,
    emotions,
    note: null,
    rev: 1,
  };
}

describe('localDayKey', () => {
  it('puts a late-evening reading on the day the person had, not the UTC day', () => {
    // 00:20 BST on the 12th is 23:20 UTC on the 11th. Slicing the stored string
    // would file this under the 11th — invisible in winter, wrong all summer.
    expect(localDayKey('2026-07-11T23:20:00Z', LONDON)).toBe('2026-07-12');
    expect(localDayKey('2026-07-11T23:20:00Z', 'UTC')).toBe('2026-07-11');
  });

  it('agrees with the stored string when the offset is zero', () => {
    expect(localDayKey('2026-01-15T12:00:00Z', LONDON)).toBe('2026-01-15');
  });
});

describe('bandsFor', () => {
  it('weights each check-in equally, not each tag', () => {
    // One reading names six Happy chips, three name one Sad chip each. By tag
    // that is 6:3 Happy. Per check-in it is 1:3 Sad, which is the day that
    // actually happened.
    const entries = [
      doc('2026-09-01T09:00:00Z', [
        'Happy/Calm',
        'Happy/Joyful',
        'Happy/Present',
        'Happy/Savouring',
        'Happy/Thankful',
        'Happy/Valued',
      ]),
      doc('2026-09-01T12:00:00Z', ['Sad/Low']),
      doc('2026-09-01T15:00:00Z', ['Sad/Low']),
      doc('2026-09-01T18:00:00Z', ['Sad/Low']),
    ];
    const bands = bandsFor(entries);
    expect(bands.map((b) => b.core)).toEqual(['Sad', 'Happy']);
    expect(bands[0].fraction).toBeCloseTo(0.75);
    expect(bands[1].fraction).toBeCloseTo(0.25);
  });

  it('splits one check-in across the families it names', () => {
    const bands = bandsFor([doc('2026-09-01T09:00:00Z', ['Happy/Calm', 'Sad/Low'])]);
    expect(bands.map((b) => b.fraction)).toEqual([0.5, 0.5]);
  });

  it('sums to one', () => {
    const bands = bandsFor([
      doc('2026-09-07T09:00:00Z', ['Happy/Mending', 'Sad/Fragile', 'Angry/Annoyed']),
      doc('2026-09-07T14:00:00Z', ['Fearful/Overwhelmed', 'Bad/Pressured']),
    ]);
    expect(bands.reduce((n, b) => n + b.fraction, 0)).toBeCloseTo(1);
  });

  it('carries the family hue the picker uses', () => {
    expect(bandsFor([doc('2026-09-01T09:00:00Z', ['Happy/Calm'])])[0].color).toBe('happy');
    expect(bandsFor([doc('2026-09-01T09:00:00Z', ['Bad/Drained'])])[0].color).toBe('bad');
  });

  it('ignores a word no longer in the vocabulary rather than inventing a family', () => {
    // A tag from a retired wheel revision must not colour a day. Being dropped
    // here is right; `tokens` still carries it, so nothing is lost.
    expect(bandsFor([doc('2026-09-01T09:00:00Z', ['Surprised/In awe'])])).toEqual([]);
    expect(bandsFor([doc('2026-09-01T09:00:00Z', ['Happy/Calm', 'Angry/Fed up'])])).toEqual([
      { core: 'Happy', color: 'happy', fraction: 1 },
    ]);
  });

  it('is empty for readings that tagged nothing', () => {
    expect(bandsFor([doc('2026-09-01T09:00:00Z', [])])).toEqual([]);
  });

  it('survives a stored doc with no emotions field at all', () => {
    // ⚠ Not hypothetical. `emotions` is absent from the RxDB schema's `required`
    // list, so a local doc can lack it entirely — and `WellbeingDoc` typing it
    // as `string[]` is the thing that is wrong. Every fixture here set the
    // field, so the whole suite passed while the live calendar threw
    // "emotions is not iterable" and rendered nothing at all.
    const bare = { ...doc('2026-09-01T09:00:00Z', []) } as Partial<WellbeingDoc>;
    delete bare.emotions;
    expect(bandsFor([bare as WellbeingDoc])).toEqual([]);
    const months = buildCalendar([bare as WellbeingDoc], LONDON);
    expect(months).toHaveLength(1);
    expect(months[0].cells.find((c) => c !== null)).toMatchObject({
      checkins: 1,
      bands: [],
      tokens: [],
    });
  });
});

describe('buildCalendar', () => {
  const day = (months: readonly CalendarMonth[], key: string): CalendarDay | undefined =>
    months.flatMap((m) => m.cells).find((c): c is CalendarDay => c !== null && c.key === key);

  it('tells a skipped day apart from one that was checked in but not tagged', () => {
    const months = buildCalendar(
      [doc('2026-09-01T09:00:00Z', []), doc('2026-09-03T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    const first = day(months, '2026-09-01')!;
    const skipped = day(months, '2026-09-02')!;
    expect(first).toMatchObject({ checkins: 1, bands: [], spread: 0 });
    expect(skipped).toMatchObject({ checkins: 0, bands: [], spread: null });
    expect(skipped.scoreLow).toBeNull();
  });

  it('keeps the gap rather than closing the grid up around it', () => {
    const months = buildCalendar(
      [doc('2026-09-01T09:00:00Z', ['Happy/Calm']), doc('2026-09-05T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    expect(day(months, '2026-09-03')).toBeDefined();
  });

  it('records the score range, and zero spread for a single reading', () => {
    const months = buildCalendar(
      [
        doc('2026-09-08T09:00:00Z', ['Happy/Calm'], 45),
        doc('2026-09-08T19:00:00Z', ['Sad/Remorseful'], 30),
        doc('2026-09-09T09:00:00Z', ['Happy/Calm'], 40),
      ],
      LONDON,
    );
    expect(day(months, '2026-09-08')).toMatchObject({ scoreLow: 30, scoreHigh: 45, spread: 15 });
    expect(day(months, '2026-09-09')).toMatchObject({ spread: 0 });
  });

  it('starts weeks on Monday and pads so the 1st lands under its weekday', () => {
    // 1 September 2026 is a Tuesday, so exactly one blank precedes it. Readings
    // at both ends so the outside-the-range rule below does not also apply.
    const [sep] = buildCalendar(
      [doc('2026-09-01T09:00:00Z', ['Happy/Calm']), doc('2026-09-30T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    expect(sep.cells.slice(0, 1)).toEqual([null]);
    expect(sep.cells[1]).toMatchObject({ dayOfMonth: 1 });
    expect(sep.cells.filter((c) => c !== null)).toHaveLength(30);
  });

  it('pads the days outside the log instead of calling them skipped', () => {
    // A dashed box for the rest of the month says you missed days you have not
    // lived yet, and one before the first reading says you missed days before
    // you started. Both are padding; only a gap BETWEEN readings is a real
    // skipped day, which the gap test above pins.
    const [sep] = buildCalendar(
      [doc('2026-09-10T09:00:00Z', ['Happy/Calm']), doc('2026-09-12T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    const present = sep.cells.filter((c): c is CalendarDay => c !== null);
    expect(present.map((c) => c.dayOfMonth)).toEqual([10, 11, 12]);
    expect(present[1]).toMatchObject({ dayOfMonth: 11, checkins: 0 });
    // And the weeks that hold nothing at all are gone, rather than rendering
    // three rows of blank squares: 10–12 September is one week.
    expect(sep.cells).toHaveLength(7);
  });

  it('keeps a fully skipped week between readings', () => {
    // Trimming is for the ends only. A week you skipped in the middle is a fact
    // about the log and has to stay, or the grid silently closes time up.
    const [sep] = buildCalendar(
      [doc('2026-09-01T09:00:00Z', ['Happy/Calm']), doc('2026-09-21T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    const days = sep.cells.filter((c): c is CalendarDay => c !== null);
    expect(days).toHaveLength(21);
    expect(days.filter((d) => d.checkins === 0)).toHaveLength(19);
  });

  it('draws no range for readings that carried no usable score', () => {
    // ⚠ Also not hypothetical, and the same shape as the missing `emotions`:
    // `scoreTenths` is typed `number` and five stored docs did not have one.
    // `Math.min(...[undefined])` is NaN and `NaN === 0` is false, so the day
    // passed the "did it hold still" test and drew a bar at NaN% titled
    // `score NaN–NaN`.
    const bare = { ...doc('2026-09-01T09:00:00Z', ['Happy/Calm']) } as Partial<WellbeingDoc>;
    delete bare.scoreTenths;
    const [sep] = buildCalendar([bare as WellbeingDoc], LONDON);
    const d = sep.cells.find((c) => c !== null)!;
    expect(d).toMatchObject({ checkins: 1, scored: 0, scoreLow: null, scoreHigh: null, spread: null });
    expect(d.bands).toHaveLength(1);
  });

  it('keeps the readings that do have a score when one in the day does not', () => {
    const bare = { ...doc('2026-09-01T12:00:00Z', ['Sad/Low']) } as Partial<WellbeingDoc>;
    delete bare.scoreTenths;
    const [sep] = buildCalendar(
      [doc('2026-09-01T09:00:00Z', ['Happy/Calm'], 45), bare as WellbeingDoc],
      LONDON,
    );
    const d = sep.cells.find((c) => c !== null)!;
    expect(d).toMatchObject({ checkins: 2, scored: 1, scoreLow: 45, scoreHigh: 45, spread: 0 });
  });

  it('spans every month between the first and last reading, oldest first', () => {
    // Chronological at both scales. Opening on the newest month is the view's
    // job (it scrolls to the end), not the ordering's.
    const months = buildCalendar(
      [doc('2026-06-26T09:00:00Z', ['Happy/Calm']), doc('2026-09-11T09:00:00Z', ['Happy/Calm'])],
      LONDON,
    );
    expect(months.map((m) => m.key)).toEqual(['2026-06', '2026-07', '2026-08', '2026-09']);
  });

  it('is empty for no readings at all', () => {
    expect(buildCalendar([], LONDON)).toEqual([]);
  });

  it('keeps every tag of the day, including one it could not colour', () => {
    const months = buildCalendar(
      [
        doc('2026-09-07T09:00:00Z', ['Happy/Mending', 'Sad/Fragile']),
        doc('2026-09-07T14:00:00Z', ['Sad/Fragile', 'Surprised/In awe']),
      ],
      LONDON,
    );
    expect(day(months, '2026-09-07')!.tokens).toEqual([
      'Happy/Mending',
      'Sad/Fragile',
      'Surprised/In awe',
    ]);
  });
});

describe('tokensAcross', () => {
  it('merges selected days without repeating a word', () => {
    const months = buildCalendar(
      [
        doc('2026-09-07T09:00:00Z', ['Happy/Steady', 'Sad/Fragile']),
        doc('2026-09-08T09:00:00Z', ['Sad/Fragile', 'Bad/Exhausted']),
      ],
      LONDON,
    );
    const days = months
      .flatMap((m) => m.cells)
      .filter((c): c is CalendarDay => c !== null && c.checkins > 0);
    expect(tokensAcross(days)).toEqual([
      'Happy/Steady',
      'Sad/Fragile',
      'Bad/Exhausted',
    ]);
  });
});
