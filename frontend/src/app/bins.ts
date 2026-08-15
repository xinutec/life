import { BinDay } from './models';

/** A collection day: everything that goes out on one morning, and how soon. */
export interface BinCollection {
  /** `YYYY-MM-DD`, as the council states it. */
  date: string;
  /** What goes out, shortened for reading — see {@link shortKind}. */
  kinds: string[];
  /** "today" / "tomorrow" / "in 3 days" / "Thu 20 Aug". */
  when: string;
  /** Today or tomorrow — the case worth a glance rather than a scroll. The
   *  bins are a thing you act on the night before, so "soon" that includes
   *  next week would make the highlight meaningless. */
  imminent: boolean;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** The council suffixes every name with " collection" — true, and noise in a
 *  list already headed "Bins". Only the exact suffix is dropped, so a name that
 *  does not end that way survives intact rather than being trimmed on a guess. */
export function shortKind(kind: string): string {
  const suffix = ' collection';
  return kind.endsWith(suffix) ? kind.slice(0, -suffix.length) : kind;
}

/** The next collection days, soonest first, grouped so one morning is one row.
 *
 *  Grouped because that is how it happens: three bins go out together, and
 *  three rows saying the same date is a list you have to read to learn one
 *  thing. `now` is injectable for tests, as in `expiry.ts`. */
export function nextCollections(days: BinDay[], now: Date = new Date()): BinCollection[] {
  const byDate = new Map<string, string[]>();
  for (const d of days) {
    const kinds = byDate.get(d.date);
    if (kinds) kinds.push(shortKind(d.kind));
    else byDate.set(d.date, [shortKind(d.kind)]);
  }
  return [...byDate.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([date, kinds]) => {
      const days = daysUntil(date, now);
      return { date, kinds, when: when(date, days), imminent: days !== null && days <= 1 };
    });
}

/** Whole days from `now` to a `YYYY-MM-DD`, or `null` if it is not a date.
 *
 *  ⚠ **Two civil dates, compared as civil dates.** The council states a bare day
 *  with no timezone, so it is read at UTC midnight and never shifts — that part
 *  was always right. What "today" means is the other half, and it is the
 *  READER'S day, not Greenwich's: taking it from `now.getUTCDate()` put the app
 *  a whole day behind between midnight and 01:00 BST, where the local date has
 *  turned over and the UTC one has not. A collection happening that morning read
 *  as "tomorrow". A bin you are told about the morning after is a bin you missed.
 *
 *  Local fields encoded through `Date.UTC` so both sides are the same kind of
 *  thing: a calendar day, with no hours in it to be wrong about. That also keeps
 *  the case the old comment was guarding — someone west of Greenwich in the
 *  evening, whose UTC date has already advanced past their own. Both ends of the
 *  world are the same mistake, and neither happens once the comparison is civil. */
function daysUntil(date: string, now: Date): number | null {
  const at = new Date(`${date}T00:00:00Z`);
  if (Number.isNaN(at.getTime())) return null;
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((at.getTime() - today) / DAY_MS);
}

function when(date: string, days: number | null): string {
  if (days === null) return date;
  if (days <= 0) return 'today';
  if (days === 1) return 'tomorrow';
  if (days < 7) return `in ${days} days`;
  return new Date(`${date}T00:00:00Z`).toLocaleDateString('en-GB', {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
  });
}
