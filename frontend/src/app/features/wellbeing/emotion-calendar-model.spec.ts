import { describe, expect, it } from 'vitest';

import { WellbeingDoc } from '../../sync/wellbeing-store';
import {
  CalendarDay,
  CalendarMonth,
  bandsFor,
  buildCalendar,
  localDayKey,
  tallyAcross,
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
  // One word in the morning, three in the evening: weighting each check-in
  // equally would make that half bad; pooled, it is three quarters good.
  it('pools words across the day, so a one-word morning does not own half of it', () => {
    const bands = bandsFor([
      doc('2026-08-27T09:11:07Z', ['Bad/Sleepy']),
      doc('2026-08-27T17:22:59Z', ['Happy/Caring', 'Happy/Calm', 'Happy/Present']),
    ]);
    expect(bands).toEqual([
      { core: 'Happy', color: 'happy', fraction: 0.75 },
      { core: 'Bad', color: 'bad', fraction: 0.25 },
    ]);
  });

  // The cost of the rule above, asserted so it is a decision and not a surprise:
  // a check-in carrying more words carries more of the day.
  it('lets a reading with more words count for more of the day', () => {
    const bands = bandsFor([
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
    ]);
    // Six happy words against three sad ones, not one reading against three.
    expect(bands.map((b) => b.core)).toEqual(['Happy', 'Sad']);
    expect(bands[0].fraction).toBeCloseTo(6 / 9);
    expect(bands[1].fraction).toBeCloseTo(3 / 9);
  });

  it('splits one check-in across the families it names', () => {
    const bands = bandsFor([doc('2026-09-01T09:00:00Z', ['Happy/Calm', 'Sad/Low'])]);
    expect(bands.map((b) => b.fraction)).toEqual([0.5, 0.5]);
  });

  it('weights a single reading by how many words name each family', () => {
    // Three happy words and one sad one is three quarters happy, not half and
    // half by distinct family.
    const bands = bandsFor([
      doc('2026-09-01T09:00:00Z', ['Happy/Calm', 'Happy/Joyful', 'Happy/Present', 'Sad/Low']),
    ]);
    expect(bands).toEqual([
      { core: 'Happy', color: 'happy', fraction: 0.75 },
      { core: 'Sad', color: 'sad', fraction: 0.25 },
    ]);
  });

  it('does not let a retired word dilute the families still in the wheel', () => {
    // The dropped token must not count toward the denominator: two live words
    // beside one retired one is 2/2, not 2/3 with a third of the box unpainted.
    const bands = bandsFor([
      doc('2026-09-01T09:00:00Z', ['Happy/Calm', 'Sad/Low', 'Angry/Fed up']),
    ]);
    expect(bands.reduce((n, b) => n + b.fraction, 0)).toBeCloseTo(1);
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
    // The RxDB schema does not require `emotions`, so a stored doc can lack it.
    const bare = { ...doc('2026-09-01T09:00:00Z', []) } as Partial<WellbeingDoc>;
    delete bare.emotions;
    expect(bandsFor([bare as WellbeingDoc])).toEqual([]);
    const months = buildCalendar([bare as WellbeingDoc], LONDON, '2026-09-01');
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
      '2026-09-03',
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
      '2026-09-05',
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
      '2026-09-09',
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
      '2026-09-30',
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
      '2026-09-12',
    );
    const present = sep.cells.filter((c): c is CalendarDay => c !== null);
    expect(present.map((c) => c.dayOfMonth)).toEqual([10, 11, 12]);
    expect(present[1]).toMatchObject({ dayOfMonth: 11, checkins: 0 });
    // And the weeks that hold nothing at all are gone, rather than rendering
    // three rows of blank squares: 10–12 September is one week.
    expect(sep.cells).toHaveLength(7);
  });

  it('keeps today on the grid before the day has its first check-in', () => {
    // Today is neither "before you started" nor "not happened yet", so it gets a
    // square before its first check-in.
    const [sep] = buildCalendar([doc('2026-09-10T09:00:00Z', ['Happy/Calm'])], LONDON, '2026-09-12');
    const present = sep.cells.filter((c): c is CalendarDay => c !== null);
    expect(present.map((c) => c.dayOfMonth)).toEqual([10, 11, 12]);
    expect(present[2]).toMatchObject({ dayOfMonth: 12, checkins: 0 });
  });

  it('stops at today rather than filling the rest of the month', () => {
    // The other half, and why this is a `max` and not "extend to the end of the
    // month": drawing the 13th to the 30th would claim eighteen days you have
    // not lived.
    const [sep] = buildCalendar([doc('2026-09-10T09:00:00Z', ['Happy/Calm'])], LONDON, '2026-09-12');
    expect(sep.cells.filter((c) => c !== null)).toHaveLength(3);
  });

  it('opens the current month when the log stopped in an earlier one', () => {
    // A month with no readings at all still has to exist once today is in it,
    // or the calendar's last page is a month you have already left.
    const months = buildCalendar([doc('2026-08-31T09:00:00Z', ['Happy/Calm'])], LONDON, '2026-09-02');
    expect(months.map((m) => m.key)).toEqual(['2026-08', '2026-09']);
    const sep = months[1].cells.filter((c): c is CalendarDay => c !== null);
    expect(sep.map((c) => c.dayOfMonth)).toEqual([1, 2]);
    expect(sep.every((c) => c.checkins === 0)).toBe(true);
  });

  it('keeps a reading dated after today rather than trimming it away', () => {
    // `last` is the LATER of the two, not `today`. A device with a fast clock,
    // or a document synced from one, must not vanish from the grid.
    const [sep] = buildCalendar([doc('2026-09-14T09:00:00Z', ['Happy/Calm'])], LONDON, '2026-09-12');
    expect(sep.cells.filter((c) => c !== null)).toHaveLength(1);
  });

  it('stays empty for someone who has never checked in', () => {
    // Today extends a log; it does not invent one. No readings must still mean
    // no calendar, rather than a single lonely square.
    expect(buildCalendar([], LONDON, '2026-09-12')).toEqual([]);
  });

  it('keeps a fully skipped week between readings', () => {
    // Trimming is for the ends only. A week you skipped in the middle is a fact
    // about the log and has to stay, or the grid silently closes time up.
    const [sep] = buildCalendar(
      [doc('2026-09-01T09:00:00Z', ['Happy/Calm']), doc('2026-09-21T09:00:00Z', ['Happy/Calm'])],
      LONDON,
      '2026-09-21',
    );
    const days = sep.cells.filter((c): c is CalendarDay => c !== null);
    expect(days).toHaveLength(21);
    expect(days.filter((d) => d.checkins === 0)).toHaveLength(19);
  });

  it('draws no range for readings that carried no usable score', () => {
    // Some stored docs have no `scoreTenths`; `Math.min(...[undefined])` is NaN.
    const bare = { ...doc('2026-09-01T09:00:00Z', ['Happy/Calm']) } as Partial<WellbeingDoc>;
    delete bare.scoreTenths;
    const [sep] = buildCalendar([bare as WellbeingDoc], LONDON, '2026-09-01');
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
      '2026-09-01',
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
      '2026-09-11',
    );
    expect(months.map((m) => m.key)).toEqual(['2026-06', '2026-07', '2026-08', '2026-09']);
  });

  it('is empty for no readings at all', () => {
    expect(buildCalendar([], LONDON, '2026-09-12')).toEqual([]);
  });

  it('keeps every tag of the day, including one it could not colour', () => {
    const months = buildCalendar(
      [
        doc('2026-09-07T09:00:00Z', ['Happy/Mending', 'Sad/Fragile']),
        doc('2026-09-07T14:00:00Z', ['Sad/Fragile', 'Surprised/In awe']),
      ],
      LONDON,
      '2026-09-07',
    );
    expect(day(months, '2026-09-07')!.tokens).toEqual([
      'Happy/Mending',
      'Sad/Fragile',
      'Surprised/In awe',
    ]);
  });
});

describe('tallyAcross', () => {
  const selectedFrom = (docs: Parameters<typeof buildCalendar>[0]): CalendarDay[] =>
    buildCalendar(docs, LONDON, '2026-09-30')
      .flatMap((m) => m.cells)
      .filter((c): c is CalendarDay => c !== null && c.checkins > 0);

  it('merges selected days without repeating a word', () => {
    const tally = tallyAcross(
      selectedFrom([
        doc('2026-09-07T09:00:00Z', ['Happy/Steady', 'Sad/Fragile']),
        doc('2026-09-08T09:00:00Z', ['Sad/Fragile', 'Bad/Exhausted']),
      ]),
    );
    expect(tally.map((t) => t.token).sort()).toEqual([
      'Bad/Exhausted',
      'Happy/Steady',
      'Sad/Fragile',
    ]);
  });

  // The panel bounds its height and scrolls, so this order decides what a
  // reader never sees. A word on more days outranks one on fewer, whatever
  // order the days arrived in.
  it('puts the word covering most days first', () => {
    const tally = tallyAcross(
      selectedFrom([
        doc('2026-09-07T09:00:00Z', ['Sad/Fragile']),
        doc('2026-09-08T09:00:00Z', ['Sad/Fragile', 'Happy/Steady']),
        doc('2026-09-09T09:00:00Z', ['Sad/Fragile']),
      ]),
    );
    expect(tally).toEqual([
      { token: 'Sad/Fragile', days: 3 },
      { token: 'Happy/Steady', days: 1 },
    ]);
  });

  // Counting readings instead of days would say 3, and would rank a day you
  // happened to log three times above a feeling that actually recurred.
  it('counts a word once per day however many check-ins named it', () => {
    const tally = tallyAcross(
      selectedFrom([
        doc('2026-09-07T08:00:00Z', ['Happy/Steady']),
        doc('2026-09-07T13:00:00Z', ['Happy/Steady']),
        doc('2026-09-07T20:00:00Z', ['Happy/Steady']),
      ]),
    );
    expect(tally).toEqual([{ token: 'Happy/Steady', days: 1 }]);
  });

  // Equal counts must not reshuffle as days are added, or the list appears to
  // jump under the reader's finger.
  it('breaks ties on the token so the order is stable', () => {
    const tally = tallyAcross(
      selectedFrom([doc('2026-09-07T09:00:00Z', ['Sad/Fragile', 'Bad/Exhausted', 'Happy/Steady'])]),
    );
    expect(tally.map((t) => t.token)).toEqual(['Bad/Exhausted', 'Happy/Steady', 'Sad/Fragile']);
  });

  it('is empty for no selection', () => {
    expect(tallyAcross([])).toEqual([]);
  });
});
