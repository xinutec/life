/** Expiry display: the raw ISO date is the *storage* format; what the user
 *  needs at a glance is urgency — expired, about to, or fine. */

export interface ExpiryInfo {
  label: string;
  /** CSS hook: 'expired' | 'soon' | 'ok'. */
  cls: 'expired' | 'soon' | 'ok';
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Human urgency for a `YYYY-MM-DD` expiry. `now` is injectable for tests. */
export function expiryInfo(expiry: string, now: Date = new Date()): ExpiryInfo {
  const date = new Date(`${expiry}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return { label: expiry, cls: 'ok' };
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
  const label = date.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    timeZone: 'UTC',
  });
  return { label, cls: 'ok' };
}
