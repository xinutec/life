import { describe, expect, it } from 'vitest';

import { fromMinorUnits, toMinorUnits } from './money';

describe('toMinorUnits', () => {
  it('reads whole and decimal prices', () => {
    expect(toMinorUnits('3')).toBe(300);
    expect(toMinorUnits('3.30')).toBe(330);
    expect(toMinorUnits('0.05')).toBe(5);
    expect(toMinorUnits('12.99')).toBe(1299);
  });

  it('pads a single decimal place rather than reading it as pence', () => {
    // "3.3" is three pounds thirty, not three pounds three. Reading the digit
    // as written would be off by a factor of ten, on every such input.
    expect(toMinorUnits('3.3')).toBe(330);
  });

  it('does not go through a float', () => {
    // 3.30 * 100 is 330.00000000000006 in binary floating point. Rounding hides
    // it for most inputs and fails for some, which is the worst kind of money
    // bug: a penny out, for reasons nobody can reproduce.
    for (const [text, minor] of [
      ['1.15', 115],
      ['1.16', 116],
      ['2.03', 203],
      ['8.29', 829],
      ['19.99', 1999],
      ['1.005', null],
    ] as const) {
      expect(toMinorUnits(text)).toBe(minor);
    }
  });

  it('accepts a currency symbol and a comma decimal', () => {
    expect(toMinorUnits('£3.30')).toBe(330);
    expect(toMinorUnits(' 3,30 ')).toBe(330);
  });

  it('refuses anything that is not a price, rather than guessing', () => {
    // A wrong number sits in the spending history looking exactly like a real
    // one; an absent one is visibly missing.
    for (const bad of ['', '  ', 'free', '3.333', '-3', '3.', '.3', '1e3', '3 30']) {
      expect(toMinorUnits(bad)).toBeNull();
    }
  });
});

describe('fromMinorUnits', () => {
  it('renders minor units back, keeping both decimal places', () => {
    expect(fromMinorUnits(330)).toBe('3.30');
    expect(fromMinorUnits(5)).toBe('0.05');
    expect(fromMinorUnits(300)).toBe('3.00');
    expect(fromMinorUnits(1299)).toBe('12.99');
  });

  it('round-trips every price the parser accepts', () => {
    for (const text of ['0.01', '3.30', '12.99', '100.00']) {
      const minor = toMinorUnits(text);
      expect(minor).not.toBeNull();
      expect(fromMinorUnits(minor!)).toBe(text);
    }
  });
});
