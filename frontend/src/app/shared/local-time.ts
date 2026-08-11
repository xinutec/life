/** Crossing between an instant and `<input type="datetime-local">`.
 *
 *  The input has no timezone in it at all — it is wall-clock text, and the
 *  browser neither reads nor writes an offset. So both directions have to be
 *  done deliberately: `new Date(iso).toISOString().slice(0, 16)` looks like the
 *  right answer and puts UTC in a field the user reads as their own clock,
 *  which in British Summer Time schedules everything an hour early.
 *
 *  Extracted so the two screens that take a time can't drift apart on it.
 */

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
 *
 *  A bare `YYYY-MM-DDTHH:mm` is read as local time by the language itself
 *  (unlike a date-only string, which is read as UTC) — which is exactly what
 *  the field means, so there is nothing to correct here, only something not to
 *  break.
 *
 *  The shape is checked BEFORE parsing, because `new Date()` falls back to an
 *  implementation-defined parser that is far more willing than the spec: it
 *  reads the half-typed `"2026-08-"` as a real date, which would let a
 *  mid-keystroke field submit a time nobody chose. Empty, partial and
 *  impossible (`02-31`) values are all null. */
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
