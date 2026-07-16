/** Display names for product-data sources ('off', a shop id, 'user'), shared by
 *  the product picker and the product page so a source never reads differently
 *  on two screens. Explicit map — a new source gets a name here, not a clever
 *  title-casing that guesses wrong. */
export function sourceLabel(source: string | null): string {
  switch (source) {
    case 'off':
      return 'Open Food Facts';
    case 'asda':
      return 'Asda';
    case 'waitrose':
      return 'Waitrose';
    case 'user':
      return 'added by you';
    case null:
      return '';
    default:
      return source;
  }
}
