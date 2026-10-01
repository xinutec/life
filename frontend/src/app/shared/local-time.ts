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

/** The input's local wall-clock text → an ISO instant, or null if it isn't one.
 *  `YYYY-MM-DDTHH:mm` parses as local time, as the field means. Parsing alone
 *  is not enough: `new Date()` accepts half-typed input like `"2026-08-"`, and
 *  an impossible day ROLLS OVER rather than failing, so 31 February becomes
 *  3 March, a shop trip on a day nobody chose. The instant must print back as
 *  exactly the text that was typed. */
export function fromLocalInput(value: string): string | null {
  const d = new Date(value);
  return toLocalInput(d) === value.slice(0, 16) ? d.toISOString() : null;
}
