/** How much of something, written the one way the app writes it: "950 g",
 *  "1 bottle", or just "3" when the thing is countable and has no unit.
 *
 *  Shared because the alternative had already happened. The inventory list
 *  spaced them and the use-sheet did not, which reads fine for the units that
 *  are symbols ("950g") and badly for the ones that are words ("1bottle") — and
 *  an item's unit is free text, so both kinds are always in play. One function
 *  means a new screen cannot pick the wrong one.
 *
 *  Returns `''` for an unmeasured row, which is most of them: an item with no
 *  quantity is not tracking an amount, and printing a bare unit would claim it
 *  holds some unspecified number of them. */
export function amount(quantity: number | null, unit: string | null): string {
  if (quantity == null) return '';
  const u = unit?.trim();
  return u ? `${quantity} ${u}` : `${quantity}`;
}
