import { BinDay } from './models';
import { daysUntil } from './shared/civil-day';

/** Everything that goes out on one morning. */
export interface BinCollection {
  date: string;
  kinds: string[];
  /** "today" / "tomorrow" / "in 3 days" / "Thu 20 Aug". */
  when: string;
  /** Today or tomorrow: bins are put out the night before. */
  imminent: boolean;
}

/** Drops the council's " collection" suffix. */
export function shortKind(kind: string): string {
  const suffix = ' collection';
  return kind.endsWith(suffix) ? kind.slice(0, -suffix.length) : kind;
}

/** The next collection days, soonest first, one row per morning. */
export function nextCollections(days: BinDay[], now: Date = new Date()): BinCollection[] {
  const byDate = new Map<string, string[]>();
  for (const d of days) {
    const kinds = byDate.get(d.date);
    if (kinds) kinds.push(shortKind(d.kind));
    else byDate.set(d.date, [shortKind(d.kind)]);
  }
  return [...byDate.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([date, kinds]) => {
      const days = daysUntil(date, now);
      return { date, kinds, when: when(date, days), imminent: days !== null && days <= 1 };
    });
}

function when(date: string, days: number | null): string {
  if (days === null) return date;
  if (days <= 0) return 'today';
  if (days === 1) return 'tomorrow';
  if (days < 7) return `in ${days} days`;
  return new Date(`${date}T00:00:00Z`).toLocaleDateString('en-GB', {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
  });
}
