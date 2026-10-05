/** "today" / "yesterday" / "n days ago", or a date past a week. Days are elapsed
 *  24-hour periods, not calendar days (ago.spec.ts). */
export function ago(epochMs: number, now: number = Date.now()): string {
  const days = Math.floor((now - epochMs) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(epochMs).toLocaleDateString();
}
