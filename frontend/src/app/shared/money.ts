/** Minor units per major unit for the currencies this app takes. GBP, and the
 *  two-decimal majority, are the only cases here; a zero-decimal currency (JPY)
 *  would need this to become a lookup rather than a constant. */
const MINOR_PER_MAJOR = 100;
const DECIMALS = 2;

/**
 * Read a typed price ("3.30", "£3.30", "3") into integer minor units, or
 * `null` if it isn't one: a wrong price looks real, a missing one shows.
 *
 * Never via `parseFloat` (`3.30 * 100` is `330.00000000000006`): both sides of
 * the point are read as integers.
 */
export function toMinorUnits(text: string): number | null {
  const trimmed = text.trim().replace(/^[£$€]/, '').trim();
  if (trimmed === '') return null;
  // One optional decimal point, at most two places after it. More places is a
  // rejection rather than a rounding: nobody paid £3.333, so the input is a
  // typo and guessing which digit was meant is not this function's business.
  const m = /^(\d+)(?:[.,](\d{1,2}))?$/.exec(trimmed);
  if (!m) return null;
  const major = Number(m[1]);
  // Pad so "3.3" is thirty pence, not three: the trailing zero is implied by
  // position, and reading it as written would be off by a factor of ten.
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

/** Minor units with their currency: "£3.30", or "3.30 EUR" for any other. */
export function formatMoney(minor: number, currency: string): string {
  const amount = fromMinorUnits(minor);
  return currency === 'GBP' ? `£${amount}` : `${amount} ${currency}`;
}
