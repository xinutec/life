import { describe, expect, it } from 'vitest';

import { BuyIdentity, ShoppingDoc, matchesIdentity, migrationStrategies } from './shopping-store';

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

const doc = (over: Partial<ShoppingDoc>): ShoppingDoc => ({
  ulid: 'u1',
  id: null,
  name: 'Yoghurt',
  quantity: null,
  unit: null,
  barcode: null,
  category: 'food',
  product_id: null,
  done: false,
  rev: 0,
  ...over,
});

const identity = (over: Partial<BuyIdentity>): BuyIdentity => ({
  name: 'Yoghurt',
  barcode: null,
  product_id: null,
  ...over,
});

/** The Inventory→Buy dedupe: strongest key wins, null keys never match null. */
describe('matchesIdentity', () => {
  it('matches on the catalog link even when the names differ', () => {
    expect(
      matchesIdentity(doc({ name: 'Nomadic lassi', product_id: 7 }), identity({ product_id: 7 })),
    ).toBe(true);
  });

  it('matches on the barcode even when the names differ', () => {
    expect(
      matchesIdentity(
        doc({ name: 'Lassi', barcode: '5029617001045' }),
        identity({ barcode: '5029617001045' }),
      ),
    ).toBe(true);
  });

  it('matches on the name, case- and whitespace-insensitively', () => {
    expect(matchesIdentity(doc({ name: ' milk ' }), identity({ name: 'Milk' }))).toBe(true);
  });

  it('two barcodeless, unlinked rows with different names are different things', () => {
    expect(matchesIdentity(doc({ name: 'Milk' }), identity({ name: 'Oat milk' }))).toBe(false);
  });

  it('a null product_id never matches a null product_id', () => {
    // Both unlinked is NOT evidence of sameness — the name has to say so.
    expect(matchesIdentity(doc({ name: 'Milk' }), identity({ name: 'Beans' }))).toBe(false);
  });

  it('different catalog links fall through to the name, not to false', () => {
    // A Waitrose yoghurt and an OFF yoghurt may be the same thing under two
    // catalog ids; the weaker keys still get their say.
    expect(matchesIdentity(doc({ product_id: 3 }), identity({ product_id: 9 }))).toBe(true);
  });
});
