import { describe, expect, it } from 'vitest';

import { isRecord, numberField, stringField } from './narrow';

/** These are the boundary guards — every untrusted blob in the app comes through
 *  them — and until 2026-09-12 they had no test of their own. */
describe('isRecord', () => {
  it('accepts a plain object', () => {
    expect(isRecord({})).toBe(true);
    expect(isRecord({ a: 1 })).toBe(true);
  });

  it('rejects an ARRAY, which is the case that cost an afternoon', () => {
    // `typeof [] === 'object'` and it is not null, so an array used to satisfy a
    // guard named `isRecord`. The sync pull then read `body['documents']` off an
    // array, got undefined, and reported a malformed batch — describing the
    // symptom rather than "this is a list, not a batch".
    expect(isRecord([])).toBe(false);
    expect(isRecord([1, 2])).toBe(false);
  });

  it('rejects null and the primitives', () => {
    expect(isRecord(null)).toBe(false);
    expect(isRecord(undefined)).toBe(false);
    expect(isRecord('x')).toBe(false);
    expect(isRecord(3)).toBe(false);
  });
});

describe('stringField / numberField', () => {
  it('reads a field of the right type', () => {
    expect(stringField({ a: 'x' }, 'a')).toBe('x');
    expect(numberField({ a: 3 }, 'a')).toBe(3);
  });

  it('gives null for the wrong type, a missing key, or a non-record', () => {
    expect(stringField({ a: 3 }, 'a')).toBeNull();
    expect(numberField({ a: 'x' }, 'a')).toBeNull();
    expect(numberField({}, 'a')).toBeNull();
    expect(numberField(null, 'a')).toBeNull();
    expect(numberField([], 'a')).toBeNull();
  });

  it('keeps ZERO, which a truthiness check would drop', () => {
    // A checkpoint of rev 0 is a real value — a fresh device syncing from the
    // start — and `value || null` would turn it into "malformed".
    expect(numberField({ rev: 0 }, 'rev')).toBe(0);
  });
});
