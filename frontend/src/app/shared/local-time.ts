/** Converting between an instant and `<input type="datetime-local">`, whose
 *  text is local wall-clock with no offset. `toISOString().slice(0, 16)` would
 *  put UTC there, an hour off in British Summer Time. */

const pad = (n: number): string => String(n).padStart(2, '0');

/** An instant → the local wall-clock text the input expects. */
export function toLocalInput(instant: string | Date): string {
  const d = instant instanceof Date ? instant : new Date(instant);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

/** Exactly what the field emits, and nothing else. */
const WALL_CLOCK = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$/;

/** The input's local wall-clock text → an ISO instant, or null if it isn't one.
 *  `YYYY-MM-DDTHH:mm` parses as local time, as the field means. The shape is
 *  checked first because `new Date()` accepts half-typed input like
 *  `"2026-08-"`; empty, partial and impossible (`02-31`) values are null. */
export function fromLocalInput(value: string): string | null {
  const text = value.trim();
  if (!WALL_CLOCK.test(text)) return null;
  const d = new Date(text);
  if (Number.isNaN(d.getTime())) return null;
  // And the date has to be the one that was typed. An impossible day does not
  // come back as `Invalid Date` — it ROLLS OVER, so 31 February quietly becomes
  // 3 March, which as a shop trip is a real appointment on a day nobody chose.
  return toLocalInput(d) === text.slice(0, 16) ? d.toISOString() : null;
}
