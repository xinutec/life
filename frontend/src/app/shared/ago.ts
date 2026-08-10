/** Epoch millis → "today" / "yesterday" / "n days ago" / a date.
 *
 *  Recency is the point, not precision: a shelf price observed weeks ago should
 *  read as stale, and an event from this morning should read as this morning.
 *  Past a week the relative form stops helping ("43 days ago" is not a date
 *  anybody can place), so it becomes an actual date.
 *
 *  Shared because two screens phrase the same question — the product page's
 *  price/document freshness and an item's history — and two spellings of "3
 *  days ago" in one app is the kind of difference a reader has to stop and
 *  decide is meaningless. */
export function ago(epochMs: number): string {
  const days = Math.floor((Date.now() - epochMs) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(epochMs).toLocaleDateString();
}
