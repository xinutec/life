import { describe, expect, it } from 'vitest';

import { canonicalBarcode } from './barcode';

describe('canonicalBarcode', () => {
  // The same real pairs as the server's tests/product_ids.rs: the two must agree.
  it.each([
    ['065928546009', '0065928546009'],
    ['0065928546009', '0065928546009'],
    ['000002684710', '02684710'],
    ['0000050378289', '50378289'],
    ['50378289', '50378289'],
    ['05054070704431', '5054070704431'],
    ['634158823732', '0634158823732'],
    ['5000112548167', '5000112548167'],
    ['7', '00000007'],
  ])('%s → %s', (given, canonical) => {
    expect(canonicalBarcode(given)).toBe(canonical);
  });

  it('leaves what is not a barcode as typed, trimmed', () => {
    expect(canonicalBarcode(' QR-thing ')).toBe('QR-thing');
    expect(canonicalBarcode('A12')).toBe('A12'); // short enough to be padded, if it were digits
    expect(canonicalBarcode('12A')).toBe('12A');
    expect(canonicalBarcode('0')).toBe('0');
  });
});
