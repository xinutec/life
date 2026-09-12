/**
 * Reading a value that came from outside the app's own types — a parsed blob, a
 * host bridge, a generic doc — without asserting what it is.
 *
 * `x as Shape` is a claim, not a check: it tells the compiler what arrived and
 * then never looks. When the claim is wrong the failure surfaces far from the
 * line that made it — as `undefined` where the types promised a value, or as
 * "[object Object]" on screen where they promised a string. Nothing in the
 * toolchain can catch that, because the assertion is the thing that lied to it.
 */

/** A value that can be indexed by string — i.e. worth asking about a field. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  // ⚠ **Arrays are excluded, and they used not to be.** `typeof [] === 'object'`
  // and it is not null, so an array satisfied a guard named `isRecord` and was
  // handed on as `Record<string, unknown>` — true of the runtime, false of the
  // name. It cost an afternoon on 2026-09-12: the e2e mock answered `[]` to a
  // sync pull, the array passed this guard, `documents` came back `undefined`
  // from an index that cannot hold it, and the failure read as a malformed
  // BATCH rather than as the wrong shape entirely. Every caller either checks
  // the field's own type straight after or asks for strings an array cannot
  // have, so no behaviour changes here — only what the predicate promises.
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
 * `Object.keys` with the key type kept.
 *
 * The lie `Object.keys` tells is a real one — it reports inherited and added
 * keys too, so `(keyof T)[]` is only sound for an object literal written right
 * here. Every caller in this app passes exactly that (a field-spec map, an
 * exhaustive `satisfies Record<Enum, …>` literal), so the claim holds; it lives
 * in this one function rather than at each callsite so there is one line to
 * revisit if that ever stops being true.
 */
export function keysOf<T extends object>(value: T): (keyof T)[] {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- see above
  return Object.keys(value) as (keyof T)[];
}
