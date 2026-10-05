import { daysBetween, localDay } from './civil-day';

/** "today" / "yesterday" / "n days ago" in the reader's calendar days, or a date
 *  past a week. */
export function ago(epochMs: number, now: number = Date.now()): string {
  const days = daysBetween(localDay(new Date(epochMs)), localDay(new Date(now)));
  if (days === null) return '';
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(epochMs).toLocaleDateString();
}
