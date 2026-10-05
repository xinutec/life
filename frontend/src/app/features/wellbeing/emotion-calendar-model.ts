/** Check-ins as calendar days. Bands are proportions and hide a bad minority,
 *  so `spread` shows whether the day held still. */
import { emotionNode } from '../../shared/emotion-wheel';
import { toPoints } from '../../shared/wellbeing-checkin';
import { WellbeingDoc } from '../../sync/wellbeing-store';

export interface CalendarBand {
  core: string;
  /** `--emo-<color>`, the picker's hue for this family. */
  color: string;
  /** 0..1, summing to 1: weighted by words, across the day's check-ins. */
  fraction: number;
}

export interface CalendarDay {
  /** In the viewer's local time. */
  key: string;
  dayOfMonth: number;
  checkins: number;
  /** Readings with a score; the score fields are null when none had one. */
  scored: number;
  /** Largest first; empty means nothing tagged, not no check-in. */
  bands: readonly CalendarBand[];
  scoreLow: number | null;
  scoreHigh: number | null;
  spread: number | null;
  /** Each emotion recorded that day, in first-seen order. */
  tokens: readonly string[];
}

export interface CalendarMonth {
  key: string;
  year: number;
  /** 0-based, as `Date` counts. */
  month: number;
  /** Leading blanks, then every day; weeks start Monday. */
  cells: readonly (CalendarDay | null)[];
}

/** Not `slice(0, 10)`, which is UTC: a summer check-in at 00:20 is the previous
 *  UTC day. */
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

/** The schema does not require `emotions`, whatever the type says. */
function tagsOf(e: WellbeingDoc): readonly string[] {
  return e.emotions ?? [];
}

/** Each word counts once, so a six-word check-in outweighs three one-word ones. */
export function bandsFor(entries: readonly WellbeingDoc[]): readonly CalendarBand[] {
  const weight = new Map<string, number>();
  // From the node, not guessed from the family's name.
  const hue = new Map<string, string>();
  let words = 0;
  for (const e of entries) {
    for (const t of tagsOf(e)) {
      const node = emotionNode(t);
      // Unknown tokens are not counted, so they cannot dilute the known ones.
      if (!node) continue;
      weight.set(node.core, (weight.get(node.core) ?? 0) + 1);
      hue.set(node.core, node.color);
      words += 1;
    }
  }
  if (!words) return [];
  return [...weight]
    .map(([core, w]) => ({ core, color: hue.get(core)!, fraction: w / words }))
    .sort((a, b) => b.fraction - a.fraction || a.core.localeCompare(b.core));
}

/** Drop empty weeks at either end; a skipped week inside is real. */
function trimEmptyWeeks(cells: readonly (CalendarDay | null)[]): (CalendarDay | null)[] {
  const weeks: (CalendarDay | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  while (weeks.length && weeks[0].every((c) => c === null)) weeks.shift();
  while (weeks.length && weeks[weeks.length - 1].every((c) => c === null)) weeks.pop();
  return weeks.flat();
}

function dayFrom(key: string, entries: readonly WellbeingDoc[]): CalendarDay {
  // Some stored docs have no score, and `Math.min(undefined)` is NaN.
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

/** Months oldest first, every day from the first reading to the later of the
 *  last reading and today: a gap is a fact. */
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
  // A max: a reading dated after today (a fast clock) must still be drawn.
  const last = today > lastReading ? today : lastReading;

  const months: CalendarMonth[] = [];
  let y = Number(first.slice(0, 4));
  let m = Number(first.slice(5, 7)) - 1;
  const endY = Number(last.slice(0, 4));
  const endM = Number(last.slice(5, 7)) - 1;
  while (y < endY || (y === endY && m <= endM)) {
    const daysInMonth = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
    const lead = (new Date(Date.UTC(y, m, 1)).getUTCDay() + 6) % 7;
    const cells: (CalendarDay | null)[] = Array.from({ length: lead }, () => null);
    for (let d = 1; d <= daysInMonth; d++) {
      const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
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

export interface TokenTally {
  readonly token: string;
  readonly days: number;
}

/** Counted per day, commonest first. */
export function tallyAcross(days: readonly CalendarDay[]): readonly TokenTally[] {
  const counts = new Map<string, number>();
  for (const d of days) for (const t of d.tokens) counts.set(t, (counts.get(t) ?? 0) + 1);
  return [...counts]
    .map(([token, n]) => ({ token, days: n }))
    .sort((a, b) => b.days - a.days || a.token.localeCompare(b.token));
}

/** For the tooltip and screen reader; scores in points, not tenths. */
export function dayTitle(day: CalendarDay): string {
  if (!day.checkins) return `${day.key} — no check-in`;
  const fams = day.bands.map((b) => `${b.core} ${Math.round(b.fraction * 100)}%`).join(', ');
  const range =
    day.scoreLow === null || day.scoreHigh === null
      ? 'no score'
      : day.spread === 0
        ? `score ${toPoints(day.scoreLow)}`
        : `score ${toPoints(day.scoreLow)}–${toPoints(day.scoreHigh)}`;
  const reads = `${day.checkins} check-in${day.checkins === 1 ? '' : 's'}`;
  return `${day.key} — ${reads}, ${range}${fams ? `, ${fams}` : ', nothing tagged'}`;
}
