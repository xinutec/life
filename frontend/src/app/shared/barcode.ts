/** The server's canonical padding (`products::ids::Barcode`); anything else is
 *  returned trimmed. */
export function canonicalBarcode(code: string): string {
  const s = code.trim();
  if (!/^\d{1,14}$/.test(s)) return s;
  const digits = s.replace(/^0+/, '');
  if (digits === '') return s;
  return digits.padStart(digits.length <= 8 ? 8 : digits.length <= 13 ? 13 : 14, '0');
}
