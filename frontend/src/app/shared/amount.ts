/** "950 g", "1 bottle" or "3"; '' without a quantity. */
export function amount(quantity: number | null, unit: string | null): string {
  if (quantity == null) return '';
  const u = unit?.trim();
  return u ? `${quantity} ${u}` : `${quantity}`;
}
