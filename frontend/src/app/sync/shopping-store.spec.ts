import { describe, expect, it } from 'vitest';

import {
  BuyIdentity,
  BuyInput,
  ShoppingDoc,
  matchesIdentity,
  migrationStrategies,
  planAdditions,
} from './shopping-store';

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

const input = (over: Partial<BuyInput>): BuyInput => ({
  name: 'Cumin',
  quantity: null,
  unit: null,
  barcode: null,
  category: 'food',
  product_id: null,
  ...over,
});

/** The Recipe→Buy bridge's rule: add what isn't there, say what already was. */
describe('planAdditions', () => {
  it('adds everything to an empty list', () => {
    const { fresh, already } = planAdditions([], [input({ name: 'Cumin' }), input({ name: 'Rice' })]);
    expect(fresh.map((f) => f.name)).toEqual(['Cumin', 'Rice']);
    expect(already).toEqual([]);
  });

  it('skips what the list already has un-done', () => {
    const { fresh, already } = planAdditions(
      [doc({ name: 'rice' })],
      [input({ name: 'Cumin' }), input({ name: 'Rice' })],
    );
    expect(fresh.map((f) => f.name)).toEqual(['Cumin']);
    expect(already).toEqual(['Rice']);
  });

  it('a recipe naming the same thing twice adds one row', () => {
    const { fresh, already } = planAdditions(
      [],
      [input({ name: 'Cumin' }), input({ name: ' cumin ' })],
    );
    expect(fresh).toHaveLength(1);
    expect(already).toEqual([' cumin ']);
  });

  it('matches an existing row by its catalog link, not just its name', () => {
    const { fresh, already } = planAdditions(
      [doc({ name: 'Bart Ground Cumin 38g', product_id: 42 })],
      [input({ name: 'cumin', product_id: 42 })],
    );
    expect(fresh).toEqual([]);
    expect(already).toEqual(['cumin']);
  });

  it('carries the quantity through untouched', () => {
    // What a recipe is short of IS what to buy — unlike the inventory bridge,
    // where the number is what you own.
    const { fresh } = planAdditions([], [input({ name: 'Rice', quantity: 250, unit: 'g' })]);
    expect(fresh[0]).toMatchObject({ quantity: 250, unit: 'g' });
  });
});
