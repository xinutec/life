/**
 * A barcode in the server's canonical padding (`products::ids::Barcode`):
 * leading zeros off, then padded to 8 or 13 digits, 14 kept. So a scanned UPC-A
 * and a shop's 13-digit form of it are one string on the synced Buy list too.
 * Anything that isn't 1-14 digits, or is all zeros, is returned trimmed.
 */
export function canonicalBarcode(code: string): string {
  const s = code.trim();
  if (!/^\d{1,14}$/.test(s)) return s;
  const digits = s.replace(/^0+/, '');
  if (digits === '') return s;
  return digits.padStart(digits.length <= 8 ? 8 : digits.length <= 13 ? 13 : 14, '0');
}
