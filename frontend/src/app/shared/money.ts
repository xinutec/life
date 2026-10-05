import { UnitPrice } from '../models';

/** Two-decimal currencies only; JPY would need a lookup. */
const MINOR_PER_MAJOR = 100;
const DECIMALS = 2;

/** A typed price ("3.30", "£3.30", "3") in minor units, or null. Both sides of
 *  the point are read as integers: `3.30 * 100` is not 330. */
export function toMinorUnits(text: string): number | null {
  const trimmed = text
    .trim()
    .replace(/^[£$€]/, '')
    .trim();
  // More than two places is a typo, refused rather than rounded.
  const m = /^(\d+)(?:[.,](\d{1,2}))?$/.exec(trimmed);
  if (!m) return null;
  const major = Number(m[1]);
  // "3.3" is thirty pence.
  const minor = Number((m[2] ?? '').padEnd(DECIMALS, '0'));
  if (!Number.isSafeInteger(major) || !Number.isSafeInteger(minor)) return null;
  return major * MINOR_PER_MAJOR + minor;
}

/** Render minor units for display: 330 → "3.30". */
export function fromMinorUnits(minor: number): string {
  const sign = minor < 0 ? '-' : '';
  const abs = Math.abs(minor);
  const major = Math.floor(abs / MINOR_PER_MAJOR);
  const rest = abs % MINOR_PER_MAJOR;
  return `${sign}${major}.${String(rest).padStart(DECIMALS, '0')}`;
}

/** A per-unit rate in the shops' own form: "£8.92/KG". */
export function formatUnitPrice(u: UnitPrice, currency: string): string {
  return `${formatMoney(u.amount_minor, currency)}/${u.measure}`;
}

/** Minor units with their currency: "£3.30", or "3.30 EUR" for any other. */
export function formatMoney(minor: number, currency: string): string {
  const amount = fromMinorUnits(minor);
  return currency === 'GBP' ? `£${amount}` : `${amount} ${currency}`;
}
