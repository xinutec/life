/** Reading values from outside the app's types by checking, not asserting. */

/** An object that is not an array. */
export function isRecord(value: unknown): value is Record<string, unknown> {
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

/** `Object.keys` with the key type kept; sound only for an object literal. */
export function keysOf<T extends object>(value: T): (keyof T)[] {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- see above
  return Object.keys(value) as (keyof T)[];
}
