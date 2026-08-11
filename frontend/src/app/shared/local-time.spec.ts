import { describe, expect, it } from 'vitest';

import { fromLocalInput, toLocalInput } from './local-time';

describe('local-time', () => {
  it('round-trips an instant through the field and back', () => {
    const instant = new Date(2026, 7, 15, 10, 30);
    const text = toLocalInput(instant);
    expect(text).toBe('2026-08-15T10:30');
    expect(fromLocalInput(text)).toBe(instant.toISOString());
  });

  it('writes the local wall clock, not UTC', () => {
    // The bug this exists to prevent: `toISOString().slice(0, 16)` puts UTC in a
    // field the user reads as their own clock — an hour out all summer.
    const instant = new Date(2026, 7, 15, 10, 30);
    const text = toLocalInput(instant);
    expect(text).toBe(
      `2026-08-15T${String(instant.getHours()).padStart(2, '0')}:30`,
    );
  });

  it('reads the field as local time', () => {
    // Same wall clock in, same wall clock out — whatever the offset is.
    const iso = fromLocalInput('2026-08-15T10:30');
    expect(new Date(iso!).getHours()).toBe(10);
  });

  it('refuses a half-typed value', () => {
    // `new Date('2026-08-')` is a VALID date to V8's fallback parser, so the
    // shape has to be checked before parsing or a mid-keystroke field submits a
    // time nobody chose.
    for (const partial of ['', '  ', '2026-08-', '2026-08-15', '2026-08-15T10']) {
      expect(fromLocalInput(partial)).toBeNull();
    }
  });

  it('refuses a day that does not exist rather than rolling it over', () => {
    // `new Date('2026-02-31T10:00')` is not invalid — it is 3 March. A shop trip
    // on a day nobody chose is worse than a field that refuses to submit.
    expect(fromLocalInput('2026-02-31T10:00')).toBeNull();
    expect(fromLocalInput('2026-13-01T10:00')).toBeNull();
  });

  it('accepts the seconds some browsers add', () => {
    expect(fromLocalInput('2026-08-15T10:30:00')).not.toBeNull();
  });
});
