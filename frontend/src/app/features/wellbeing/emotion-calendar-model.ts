/** Aggregating check-ins into calendar days, where mixing becomes visible.
 *  Bands are proportions and hide a bad minority, so `spread` (the score
 *  range) separately shows whether the day held still. */
import { emotionNode } from '../../shared/emotion-wheel';
import { WellbeingDoc } from '../../sync/wellbeing-store';

/** One family's share of a day, as a fraction of 1. */
export interface CalendarBand {
  core: string;
  /** Colour key → `--emo-<color>`, the same hue the picker gives this family. */
  color: string;
  /** Share of the day, 0..1, summing to 1 across the bands. Weighted by how
   *  many words named this family across every check-in that day. */
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

/** A check-in's tags, for a doc that may not have the field at all.
 *
 *  ⚠ The RxDB schema does not require `emotions`, so a stored doc can lack it
 *  whatever `WellbeingDoc` says. Absent is the "checked in, tagged nothing"
 *  day, not an error. */
function tagsOf(e: WellbeingDoc): readonly string[] {
  return e.emotions ?? [];
}

/** Each family's share of a day (summing to 1), counting every word once
 *  across all the day's check-ins, so a one-word bad morning isn't half the
 *  day. A six-word check-in outweighs three one-word ones. */
export function bandsFor(entries: readonly WellbeingDoc[]): readonly CalendarBand[] {
  const weight = new Map<string, number>();
  // Carried from the node that named the family rather than derived from it.
  // `core.toLowerCase()` happens to equal every colour key today, so a fallback
  // that guessed would pass every test and break silently the first time a
  // family's hue stops matching its name.
  const hue = new Map<string, string>();
  let words = 0;
  for (const e of entries) {
    for (const t of tagsOf(e)) {
      const node = emotionNode(t);
      // An unknown token is not counted at all, so it cannot dilute the families
      // that ARE known — a renamed word would otherwise silently shrink the rest.
      if (!node) continue;
      weight.set(node.core, (weight.get(node.core) ?? 0) + 1);
      hue.set(node.core, node.color);
      words += 1;
    }
  }
  if (!words) return [];
  return (
    [...weight]
      .map(([core, w]) => ({ core, color: hue.get(core)!, fraction: w / words }))
      // Largest first, then by name so equal shares order stably rather than by
      // whichever family the day happened to mention first.
      .sort((a, b) => b.fraction - a.fraction || a.core.localeCompare(b.core))
  );
}

/** Drop whole weeks with nothing in them from either end.
 *
 *  Padding cells hold a square each, so the first and last months would render
 *  rows of nothing. Interior weeks are kept: a skipped week is a real one. */
function trimEmptyWeeks(cells: readonly (CalendarDay | null)[]): (CalendarDay | null)[] {
  const weeks: (CalendarDay | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  while (weeks.length && weeks[0].every((c) => c === null)) weeks.shift();
  while (weeks.length && weeks[weeks.length - 1].every((c) => c === null)) weeks.pop();
  return weeks.flat();
}

function dayFrom(key: string, entries: readonly WellbeingDoc[]): CalendarDay {
  // ⚠ Finite-only, for the reason `tagsOf` exists: some stored docs have no
  // score, and `Math.min(...[undefined])` is NaN.
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

/** Group check-ins into calendar months, oldest first (like a chat; the view
 *  scrolls to today on load).
 *
 *  Every day from the first reading to the end gets a cell, empty or not: a gap
 *  is a fact. Days outside that range are `null` padding. The end is the later
 *  of the last reading and `today`, so today has a square before its first
 *  check-in. */
export function buildCalendar(
  docs: readonly WellbeingDoc[],
  tz?: string,
  today: string = localDayKey(new Date().toISOString(), tz),
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
  const lastReading = keys[keys.length - 1];
  // String compare on `YYYY-MM-DD` is total, so `max` is the later spelling. A
  // reading dated AFTER today — a device with a fast clock, or a doc synced from
  // one — must still be drawn, which is why this is a max and not just `today`.
  const last = today > lastReading ? today : lastReading;

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

/** Every emotion across the selected days, counted per day (not per
 *  check-in), commonest first so the panel's scrolled-off part is the tail.
 *  Ties break on the token, for a stable order. */
export function tallyAcross(days: readonly CalendarDay[]): readonly TokenTally[] {
  const counts = new Map<string, number>();
  // `CalendarDay.tokens` is already distinct per day, so a straight count of
  // appearances IS a count of days.
  for (const d of days) for (const t of d.tokens) counts.set(t, (counts.get(t) ?? 0) + 1);
  return [...counts]
    .map(([token, n]) => ({ token, days: n }))
    .sort((a, b) => b.days - a.days || a.token.localeCompare(b.token));
}
