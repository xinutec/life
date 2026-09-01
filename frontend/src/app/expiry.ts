/** Expiry display: the raw ISO date is the *storage* format; what the user
 *  needs at a glance is urgency — expired, about to, or fine. */

import { ExpiryPrecision } from './models';

export interface ExpiryInfo {
  label: string;
  /** CSS hook: 'expired' | 'soon' | 'ok'. */
  cls: 'expired' | 'soon' | 'ok';
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Human urgency for a `YYYY-MM-DD` expiry. `now` is injectable for tests.
 *
 * `precision` says how much of the date was printed on the thing. A medicine box
 * carries MM/YYYY and nothing else, and the column is a DATE, so a day gets
 * invented to store it — the month's last, because a box marked 06/2028 is good
 * THROUGH June. Rendering that as "30 Jun 2028" states a day that appears
 * nowhere on the box, and counting "in 2d" through the end of the month claims
 * an overnight change that does not happen. So the month path prints a month.
 */
export function expiryInfo(
  expiry: string,
  precision: ExpiryPrecision = 'day',
  now: Date = new Date(),
): ExpiryInfo {
  const date = new Date(`${expiry}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return { label: expiry, cls: 'ok' };
  if (precision === 'month') return monthInfo(date, now);
  // ⚠ The READER'S day, not Greenwich's — see `daysUntil` in `bins.ts`, which
  // had this same line and the same fault. Between midnight and 01:00 BST the
  // UTC date is still yesterday, so food expiring today read as `in 1d`: the
  // wrong way round to be wrong about something you are deciding whether to eat.
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((date.getTime() - today) / DAY_MS);
  if (days < 0) return { label: `expired ${-days}d ago`, cls: 'expired' };
  if (days === 0) return { label: 'expires today', cls: 'soon' };
  if (days <= 3) return { label: `in ${days}d`, cls: 'soon' };
  if (days <= 14) return { label: `in ${days}d`, cls: 'ok' };
  return { label: fullDate(date), cls: 'ok' };
}

/**
 * The month-precision reading. Whole months apart, never days: the only fact is
 * which month the box named, and the reader's own month is the only thing to
 * compare it against.
 *
 * `soon` is the current month rather than a three-day window, because that is
 * the smallest span the data can distinguish. A box expiring next month has at
 * least four more weeks in it and does not deserve a warning colour.
 */
function monthInfo(date: Date, now: Date): ExpiryInfo {
  const months =
    (date.getUTCFullYear() - now.getFullYear()) * 12 + (date.getUTCMonth() - now.getMonth());
  const named = date.toLocaleDateString('en-GB', {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  });
  if (months < 0) return { label: `expired since ${named}`, cls: 'expired' };
  if (months === 0) return { label: 'expires this month', cls: 'soon' };
  return { label: named, cls: 'ok' };
}

function fullDate(date: Date): string {
  return date.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    timeZone: 'UTC',
  });
}

/**
 * The last day of `YYYY-MM`, which is how a month-precision expiry is stored.
 *
 * Day 0 of the NEXT month is the last of this one, and it is the only spelling
 * that needs no leap-year table. Returns `null` for anything that is not a month
 * — an empty date input is the ordinary case, not an error.
 */
export function monthEnd(month: string): string | null {
  const m = /^(\d{4})-(\d{2})$/.exec(month);
  if (!m) return null;
  const year = Number(m[1]);
  const mon = Number(m[2]);
  if (mon < 1 || mon > 12) return null;
  const last = new Date(Date.UTC(year, mon, 0));
  return last.toISOString().slice(0, 10);
}

/** The `YYYY-MM` an `<input type="month">` wants, from a stored expiry. */
export function toMonth(expiry: string | null): string | null {
  if (!expiry) return null;
  return /^\d{4}-\d{2}/.test(expiry) ? expiry.slice(0, 7) : null;
}
