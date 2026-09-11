/** Aggregating check-ins into calendar days.
 *
 *  A day, not a check-in, is the unit here. Measured over the whole log: 69% of
 *  individual check-ins touch a single emotion family, so a per-entry colour is
 *  one flat hue nearly every time. Rolled up to the day only 23% are, because a
 *  day collects several readings — so the day is the smallest grain at which the
 *  mixing is visible at all.
 *
 *  ⚠ Two channels, because one cannot say it. The bands answer "how did the day
 *  go", and they are a PROPORTION, which hides a significant minority: 8
 *  September 2026 reads 71% Happy — eight check-ins, most of them genuinely
 *  fine — and is also the day that ended in a decision to cut contact. Weighting
 *  differently does not fix it (by-tag gives 74%, the same picture). So `spread`
 *  answers the second question, "did it hold still", from the score range. A day
 *  that was steadily fine and a day that averaged fine are different days, and
 *  24 of 66 multi-reading days have a spread of zero, so the channel separates
 *  them rather than colouring noise. */
import { emotionNode } from '../../shared/emotion-wheel';
import { WellbeingDoc } from '../../sync/wellbeing-store';

/** One family's share of a day, as a fraction of 1. */
export interface CalendarBand {
  core: string;
  /** Colour key → `--emo-<color>`, the same hue the picker gives this family. */
  color: string;
  fraction: number;
}

export interface CalendarDay {
  /** `YYYY-MM-DD`, in the viewer's local time — the day a person had, not UTC. */
  key: string;
  /** Day of the month, which is the number in the box. */
  dayOfMonth: number;
  /** Readings on this day. 0 = the day is outside the log or was skipped. */
  checkins: number;
  /** How many of those readings carried a usable score. Can be fewer than
   *  `checkins`, and zero while `checkins` is not — `scoreLow`/`scoreHigh` are
   *  null in that case rather than NaN. */
  scored: number;
  /** Families by share, largest first. Empty when nothing was tagged, which is
   *  NOT the same as no check-in — `checkins` tells those apart. */
  bands: readonly CalendarBand[];
  scoreLow: number | null;
  scoreHigh: number | null;
  /** `scoreHigh - scoreLow`; 0 for a single reading, null for none. */
  spread: number | null;
  /** Every distinct emotion recorded that day, in the order first seen. This is
   *  what a selected day hands to anything that renders it. */
  tokens: readonly string[];
}

export interface CalendarMonth {
  /** `YYYY-MM`. */
  key: string;
  year: number;
  /** 0-based, as `Date` counts them. */
  month: number;
  /** Leading blanks so the 1st lands under its weekday, then every day of the
   *  month. Weeks start Monday. */
  cells: readonly (CalendarDay | null)[];
}

/** Local `YYYY-MM-DD` for an ISO instant.
 *
 *  Deliberately not `slice(0, 10)` on the stored string, which is UTC: a check-in
 *  at 00:20 London in summer is the previous UTC day, and would land in the wrong
 *  box — the one case where the bug is invisible in winter and appears in June. */
export function localDayKey(iso: string, tz?: string): string {
  const d = new Date(iso);
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: tz,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(d);
  return parts;
}

/** Family shares for one day's readings.
 *
 *  Weighted per CHECK-IN, not per tag: each reading contributes one unit split
 *  across the distinct families it names. A reading with six chips would
 *  otherwise outvote three readings with one, which measures how much was typed
 *  rather than how the day went. (On real data the two agree closely — 71% vs
 *  74% on the worst case — so this is chosen for being defensible, not for
 *  changing the picture.) */
/** A check-in's tags, for a doc that may not have the field at all.
 *
 *  ⚠ `WellbeingDoc` types `emotions` as `string[]`, and the RxDB schema does NOT
 *  list it in `required` — so a locally-stored doc can legitimately lack it, and
 *  the type is the thing that is wrong. `wellbeing-entry.ts` has always guarded
 *  with `?? []`; this module did not, and `for (const t of e.emotions)` threw
 *  `emotions is not iterable` against the real log while every test passed,
 *  because the fixtures all set the field. Absent is not an error: it is the
 *  "checked in, tagged nothing" day the model already has a state for. */
function tagsOf(e: WellbeingDoc): readonly string[] {
  return e.emotions ?? [];
}

export function bandsFor(entries: readonly WellbeingDoc[]): readonly CalendarBand[] {
  const weight = new Map<string, number>();
  // Carried from the node that named the family rather than derived from it.
  // `core.toLowerCase()` happens to equal every colour key today, so a fallback
  // that guessed would pass every test and break silently the first time a
  // family's hue stops matching its name.
  const hue = new Map<string, string>();
  let total = 0;
  for (const e of entries) {
    const cores = new Set<string>();
    for (const t of tagsOf(e)) {
      const node = emotionNode(t);
      if (!node) continue;
      cores.add(node.core);
      hue.set(node.core, node.color);
    }
    if (!cores.size) continue;
    const share = 1 / cores.size;
    for (const c of cores) weight.set(c, (weight.get(c) ?? 0) + share);
    total += 1;
  }
  if (!total) return [];
  return [...weight]
    .map(([core, w]) => ({ core, color: hue.get(core)!, fraction: w / total }))
    // Largest first, then by name so equal shares order stably rather than by
    // whichever family the day happened to mention first.
    .sort((a, b) => b.fraction - a.fraction || a.core.localeCompare(b.core));
}

/** Drop whole weeks with nothing in them from either end.
 *
 *  Padding cells hold a square each, so the first and last months of a log —
 *  which are mostly outside it — rendered three or four rows of nothing. The
 *  month is not the unit anybody is looking at; the days are. Interior weeks are
 *  kept whatever they hold, because a fully skipped week is a real week that was
 *  skipped. */
function trimEmptyWeeks(cells: readonly (CalendarDay | null)[]): (CalendarDay | null)[] {
  const weeks: (CalendarDay | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  while (weeks.length && weeks[0].every((c) => c === null)) weeks.shift();
  while (weeks.length && weeks[weeks.length - 1].every((c) => c === null)) weeks.pop();
  return weeks.flat();
}

function dayFrom(key: string, entries: readonly WellbeingDoc[]): CalendarDay {
  // ⚠ Finite-only, for the same reason `tagsOf` exists: the type promises a
  // number and some stored docs do not have one. `Math.min(...[undefined])` is
  // NaN, `NaN === 0` is false — so a day with an unusable score sailed past the
  // "did it hold still" test and drew a range bar at NaN% with a `score NaN–NaN`
  // tooltip. A doc with no usable score has no range to draw, which is what
  // `null` already means here.
  const scores = entries.map((e) => e.scoreTenths).filter((n) => Number.isFinite(n));
  const tokens: string[] = [];
  for (const e of entries) {
    for (const t of tagsOf(e)) if (!tokens.includes(t)) tokens.push(t);
  }
  const low = scores.length ? Math.min(...scores) : null;
  const high = scores.length ? Math.max(...scores) : null;
  return {
    key,
    dayOfMonth: Number(key.slice(8, 10)),
    checkins: entries.length,
    scored: scores.length,
    bands: bandsFor(entries),
    scoreLow: low,
    scoreHigh: high,
    spread: low === null || high === null ? null : high - low,
    tokens,
  };
}

/** Group check-ins into calendar months, **oldest month first**.
 *
 *  Chronological, like a chat: time runs one way everywhere, down the days of a
 *  month and on down the months. Newest-first months were tried and reverted —
 *  with days ascending inside a descending list of months, time ran backwards
 *  at one scale and forwards at the other, and every month boundary was a seam.
 *  Reversing the DAYS instead would have fixed the seam by breaking the most
 *  familiar layout in the app: 1 at the bottom needs the weekday header
 *  reversed too, and then a week reads right to left.
 *
 *  The reason newest-first was reached for — that the view should open on the
 *  month you care about — is solved by scrolling to the end on load instead
 *  (see emotion-calendar.ts), which costs nothing and keeps the order honest.
 *
 *  Every day BETWEEN the first and last reading gets a cell, including the ones
 *  with nothing on them — a gap is a fact about the log, and a grid that closed
 *  up around missing days would hide it.
 *
 *  ⚠ Days OUTSIDE that range are padding (`null`), not empty days. Rendered as
 *  boxes they read as "you skipped this", which is a lie in both directions: the
 *  1st to the 25th of the first month is before you started, and the rest of the
 *  current month has not happened yet. Drawing 20 dashed boxes for the remainder
 *  of September says you missed three weeks you have not lived. */
export function buildCalendar(
  docs: readonly WellbeingDoc[],
  tz?: string,
): readonly CalendarMonth[] {
  const byDay = new Map<string, WellbeingDoc[]>();
  for (const d of docs) {
    const key = localDayKey(d.recordedAt, tz);
    const list = byDay.get(key);
    if (list) list.push(d);
    else byDay.set(key, [d]);
  }
  if (!byDay.size) return [];
  const keys = [...byDay.keys()].sort();
  const first = keys[0];
  const last = keys[keys.length - 1];

  const months: CalendarMonth[] = [];
  let y = Number(first.slice(0, 4));
  let m = Number(first.slice(5, 7)) - 1;
  const endY = Number(last.slice(0, 4));
  const endM = Number(last.slice(5, 7)) - 1;
  while (y < endY || (y === endY && m <= endM)) {
    const daysInMonth = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
    // getUTCDay is 0=Sunday; weeks start Monday, so Sunday becomes the 7th slot.
    const lead = (new Date(Date.UTC(y, m, 1)).getUTCDay() + 6) % 7;
    const cells: (CalendarDay | null)[] = Array.from({ length: lead }, () => null);
    for (let d = 1; d <= daysInMonth; d++) {
      const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
      // String compare is safe and total on `YYYY-MM-DD`: fixed width, zero
      // padded, most significant first.
      cells.push(key < first || key > last ? null : dayFrom(key, byDay.get(key) ?? []));
    }
    months.push({
      key: `${y}-${String(m + 1).padStart(2, '0')}`,
      year: y,
      month: m,
      cells: trimEmptyWeeks(cells),
    });
    m += 1;
    if (m > 11) {
      m = 0;
      y += 1;
    }
  }
  return months;
}

/** One emotion across a selection, and how much of the selection it covers. */
export interface TokenTally {
  readonly token: string;
  /** How many of the selected days named it. Never zero. */
  readonly days: number;
}

/** Every emotion across a set of selected days, commonest first.
 *  The handoff to whatever renders a selection, and the panel that lists it.
 *
 *  Counted by DAY, not by check-in: three check-ins on one day all tagged Calm
 *  is one calm day, and counting readings would make a day you logged often
 *  look like a day you felt it more.
 *
 *  ⚠ The ORDER is load-bearing, not cosmetic. The panel bounds its own height
 *  and scrolls, so something is always out of sight on a wide selection — 76
 *  distinct words across 75 days, measured on the real log. Commonest-first is
 *  what makes that honest: what falls below the fold is the tail. The previous
 *  order was first-day-that-named-it, which is arbitrary to a reader, and under
 *  a cap it would have hidden a random slice.
 *
 *  Ties break on the token so the list is stable across renders rather than
 *  reshuffling as days are added. */
export function tallyAcross(days: readonly CalendarDay[]): readonly TokenTally[] {
  const counts = new Map<string, number>();
  // `CalendarDay.tokens` is already distinct per day, so a straight count of
  // appearances IS a count of days.
  for (const d of days) for (const t of d.tokens) counts.set(t, (counts.get(t) ?? 0) + 1);
  return [...counts]
    .map(([token, n]) => ({ token, days: n }))
    .sort((a, b) => b.days - a.days || a.token.localeCompare(b.token));
}
