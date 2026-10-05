import { daysUntil, utcDay } from './shared/civil-day';

export interface WarrantyInfo {
  label: string;
  cls: 'expired' | 'soon' | 'ok';
}

/** Within this, the time left says more than the end date. */
const SOON_DAYS = 90;

/** Null `until` is "none recorded", not "no warranty", so nothing is shown. */
export function warrantyInfo(until: number | null, now: Date = new Date()): WarrantyInfo | null {
  if (until === null) return null;
  const end = new Date(until);
  // DAY to DAY, not instant to instant: the purchase is stored at MIDDAY UTC
  // (see `bought_at_from` in the purchases repo), so its UTC day is its date.
  const days = daysUntil(utcDay(end), now);
  if (days === null) return null;
  if (days < 0) return { label: `warranty ended ${date(end)}`, cls: 'expired' };
  if (days === 0) return { label: 'warranty ends today', cls: 'soon' };
  if (days <= SOON_DAYS) {
    const unit = days === 1 ? 'day' : 'days';
    return { label: `under warranty for another ${days} ${unit}`, cls: 'soon' };
  }
  return { label: `under warranty until ${date(end)}`, cls: 'ok' };
}

function date(d: Date): string {
  return d.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    timeZone: 'UTC',
  });
}
