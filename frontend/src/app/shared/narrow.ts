/**
 * Reading values from outside the app's types (parsed blobs, host bridges)
 * by checking, not asserting: `x as Shape` is never checked, and a wrong
 * claim surfaces far away as `undefined` or "[object Object]".
 */

/** A value that can be indexed by string — i.e. worth asking about a field. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  // ⚠ Arrays are excluded: `typeof [] === 'object'`, and a list is not a record.
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** The named field, only if it really is a non-empty string. */
export function stringField(value: unknown, key: string): string | null {
  if (!isRecord(value)) return null;
  const field = value[key];
  return typeof field === 'string' && field !== '' ? field : null;
}

/** The named field, only if it really is a number. */
export function numberField(value: unknown, key: string): number | null {
  if (!isRecord(value)) return null;
  const field = value[key];
  return typeof field === 'number' ? field : null;
}

/**
 * `Object.keys` with the key type kept. `(keyof T)[]` is only sound for an
 * object literal (it ignores inherited and added keys); every caller passes
 * one, and this is the one place that assumption lives.
 */
export function keysOf<T extends object>(value: T): (keyof T)[] {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- see above
  return Object.keys(value) as (keyof T)[];
}
