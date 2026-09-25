/** How much of something, written the one way the app writes it: "950 g",
 *  "1 bottle", or just "3" when the thing is countable and has no unit.
 *
 *  Shared so every screen spaces word units ("1 bottle") the same way; an
 *  item's unit is free text, so symbols and words are both in play.
 *
 *  Returns `''` for an unmeasured row, which is most of them: an item with no
 *  quantity is not tracking an amount, and printing a bare unit would claim it
 *  holds some unspecified number of them. */
export function amount(quantity: number | null, unit: string | null): string {
  if (quantity == null) return '';
  const u = unit?.trim();
  return u ? `${quantity} ${u}` : `${quantity}`;
}
