import { describe, expect, it } from 'vitest';

import { showThumb } from './product-image';

describe('showThumb', () => {
  it('shows with a barcode + an image (or unknown has_image), no failure', () => {
    expect(showThumb({ barcode: '5036589255550', has_image: true }, false)).toBe(true);
    expect(showThumb({ barcode: '5036589255550' }, false)).toBe(true); // shopping rows: unknown → try
  });

  it('shows a barcodeless item linked to a shop product with an image', () => {
    // A Waitrose product has no EAN — only a catalog id, addressed by product_id.
    expect(showThumb({ barcode: null, product_id: 42, has_image: true }, false)).toBe(true);
    expect(showThumb({ barcode: null, product_id: 42, has_image: false }, false)).toBe(false);
    expect(showThumb({ barcode: null, product_id: 42, has_image: true }, true)).toBe(false);
  });

  it('hides with neither a barcode nor a product link, when has_image is false, or after a load failure', () => {
    expect(showThumb({ barcode: null, has_image: true }, false)).toBe(false);
    expect(showThumb({ barcode: null, product_id: null, has_image: true }, false)).toBe(false);
    expect(showThumb({ barcode: '5036589255550', has_image: false }, false)).toBe(false);
    expect(showThumb({ barcode: '5036589255550', has_image: true }, true)).toBe(false);
  });
});
