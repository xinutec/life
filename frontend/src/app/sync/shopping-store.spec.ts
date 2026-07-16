import { describe, expect, it } from 'vitest';

import { migrationStrategies } from './shopping-store';

/** Pure-function tests for the RxDB schema migrations. A device that hasn't
 *  opened the app since before a schema bump runs these once on next open. */
describe('shopping migrationStrategies', () => {
  it('v1 gives a pre-identity row the server-backfill defaults', () => {
    // Must match what migration 0024 gives the same row server-side —
    // category 'food', no product link — so the two sides converge without a
    // rev bump.
    const out = migrationStrategies[1]({
      ulid: 'u',
      name: 'Yoghurt',
      barcode: '5029617001045',
    });
    expect(out).toMatchObject({
      ulid: 'u',
      name: 'Yoghurt',
      barcode: '5029617001045',
      category: 'food',
      product_id: null,
    });
  });
});
