/** Calendar days as people live them. One rule, because every copy of it had
 *  to get the same two things right: "today" is the READER'S local day, not
 *  Greenwich's (a day behind between midnight and 01:00 BST), and two civil
 *  dates are compared as dates, with no hours on either side to be wrong about. */

const DAY_MS = 24 * 60 * 60 * 1000;

const pad = (n: number): string => String(n).padStart(2, '0');

/** `d`'s local calendar day, `YYYY-MM-DD`. */
export function localDay(d: Date = new Date()): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The UTC calendar day of an instant, `YYYY-MM-DD` — for an instant that
 *  stands for a date, like a purchase stored at midday UTC. */
export function utcDay(d: Date): string {
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
}

/** Whole days from one `YYYY-MM-DD` to another (negative = earlier), or null
 *  if either is not a date. */
export function daysBetween(from: string, to: string): number | null {
  const a = Date.parse(`${from}T00:00:00Z`);
  const b = Date.parse(`${to}T00:00:00Z`);
  if (Number.isNaN(a) || Number.isNaN(b)) return null;
  return Math.round((b - a) / DAY_MS);
}

/** Whole days from the reader's day at `now` to `date`, or null if `date` is
 *  not one. */
export function daysUntil(date: string, now: Date = new Date()): number | null {
  return daysBetween(localDay(now), date);
}
