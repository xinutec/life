/** An amount as the app writes it: "950 g", "1 bottle", or "3". `''` when
 *  there is no quantity, since a bare unit would claim an unknown count. */
export function amount(quantity: number | null, unit: string | null): string {
  if (quantity == null) return '';
  const u = unit?.trim();
  return u ? `${quantity} ${u}` : `${quantity}`;
}
