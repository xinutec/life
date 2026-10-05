/** Between an instant and `<input type="datetime-local">`, which is local time
 *  with no offset. */

const pad = (n: number): string => String(n).padStart(2, '0');

/** An instant → the local wall-clock text the input expects. */
export function toLocalInput(instant: string | Date): string {
  const d = instant instanceof Date ? instant : new Date(instant);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

/** The input's text as an ISO instant, or null. It must print back as typed:
 *  `new Date()` accepts half-typed input and rolls 31 February into March. */
export function fromLocalInput(value: string): string | null {
  const d = new Date(value);
  return toLocalInput(d) === value.slice(0, 16) ? d.toISOString() : null;
}
