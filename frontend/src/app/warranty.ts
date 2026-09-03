/** How a recorded warranty reads: the date is the *storage*; what somebody needs
 *  is whether it still covers them, and how long they have. */

export interface WarrantyInfo {
  label: string;
  /** CSS hook, shared with the `.expiry` grammar: 'expired' | 'soon' | 'ok'. */
  cls: 'expired' | 'soon' | 'ok';
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** The point past which naming the end date is less useful than naming the time
 *  left. Three months is about when a decision — claim it, extend it, replace
 *  the thing — becomes one worth making rather than one to defer. */
const SOON_DAYS = 90;

/**
 * Read a warranty end date. `now` is injectable for tests.
 *
 * `until` is DERIVED server-side from the purchase date plus the recorded
 * months, and is `null` when no warranty was recorded — which is most
 * purchases. That is not "no warranty": nobody recorded one, and this returns
 * `null` so nothing renders a claim either way.
 */
export function warrantyInfo(until: string | null, now: Date = new Date()): WarrantyInfo | null {
  if (!until) return null;
  const end = new Date(until);
  if (Number.isNaN(end.getTime())) return null;
  // DAY to DAY, not instant to instant. `until` is a datetime and the purchase
  // it comes from is stored at MIDDAY UTC (see `bought_at_from` in the
  // purchases repo, which picks midday precisely so no zone offset can move the
  // day). Subtracting the raw instants leaks that half-day into the count: 2
  // July to 1 August came out as 31 days, and a warranty ending today read as
  // "another 1 days".
  const endDay = Date.UTC(end.getUTCFullYear(), end.getUTCMonth(), end.getUTCDate());
  // The READER'S day on the other side, not Greenwich's — the same fault
  // `expiry.ts` and `bins.ts` both had. Between midnight and 01:00 BST the UTC
  // date is still yesterday, so anything keyed to a UTC day is a day behind.
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((endDay - today) / DAY_MS);
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
