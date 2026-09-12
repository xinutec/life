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
 *  decide is meaningless.
 *
 *  `now` is a parameter with a default so the clock is visible in the signature
 *  and a test can state the instant it means — the convention `expiryInfo`,
 *  `warrantyInfo` and `buildCalendar` already follow. Without it this was the
 *  one function in this layer that could only be tested at whatever moment the
 *  suite happened to run, which is why it had no test at all.
 *
 *  ⚠ **The unit is elapsed 24-hour periods, NOT calendar days**, and the
 *  difference is visible: something from 23:00 last night reads as "today" at
 *  09:00 this morning, because ten hours have passed. `ago.spec.ts` pins that
 *  deliberately rather than quietly. Whether it SHOULD be calendar days is a
 *  product question — the file argues for recency over precision — but it is
 *  now a choice somebody made rather than a thing nobody noticed. */
export function ago(epochMs: number, now: number = Date.now()): string {
  const days = Math.floor((now - epochMs) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(epochMs).toLocaleDateString();
}
