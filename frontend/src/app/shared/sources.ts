import type { Source } from '../models';

/** Display names for product-data sources, shared by the product picker and the
 *  product page so a source never reads differently on two screens.
 *
 *  Exhaustive over `Source` with no `default` arm: adding a shop fails to
 *  compile here (noImplicitReturns) rather than showing the raw id. */
export function sourceLabel(source: Source | null): string {
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
  }
}
