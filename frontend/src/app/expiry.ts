import { ExpiryPrecision } from './models';
import { daysUntil } from './shared/civil-day';

export interface ExpiryInfo {
  label: string;
  cls: 'expired' | 'soon' | 'ok';
}

/** How urgent a `YYYY-MM-DD` expiry is. A month-precision one (a medicine box's
 *  MM/YYYY) never shows a day the box did not state. */
export function expiryInfo(
  expiry: string,
  precision: ExpiryPrecision = 'day',
  now: Date = new Date(),
): ExpiryInfo {
  const days = daysUntil(expiry, now);
  if (days === null) return { label: expiry, cls: 'ok' };
  const date = new Date(`${expiry}T00:00:00Z`);
  if (precision === 'month') return monthInfo(date, now);
  if (days < 0) return { label: `expired ${-days}d ago`, cls: 'expired' };
  if (days === 0) return { label: 'expires today', cls: 'soon' };
  if (days <= 3) return { label: `in ${days}d`, cls: 'soon' };
  if (days <= 14) return { label: `in ${days}d`, cls: 'ok' };
  return { label: fullDate(date), cls: 'ok' };
}

/** Compared in whole months; `soon` is this month. */
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

/** The last day of `YYYY-MM`, as a month-precision expiry is stored; null for
 *  anything else. Day 0 of the next month needs no leap-year table. */
export function monthEnd(month: string): string | null {
  const m = /^(\d{4})-(\d{2})$/.exec(month);
  if (!m) return null;
  const year = Number(m[1]);
  const mon = Number(m[2]);
  if (mon < 1 || mon > 12) return null;
  const last = new Date(Date.UTC(year, mon, 0));
  // dev-lint: allow-utc-calendar-day built at UTC midnight, so its UTC day is the day
  return last.toISOString().slice(0, 10);
}

/** For `<input type="month">`. */
export function toMonth(expiry: string | null): string | null {
  if (!expiry) return null;
  return /^\d{4}-\d{2}/.test(expiry) ? expiry.slice(0, 7) : null;
}
