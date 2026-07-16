import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';

/** A shopping row as stored locally. `ulid` is the stable identity; `rev` is the
 *  last server revision seen (set by sync, not by local edits); `id` is the
 *  server autoincrement (null until synced) used only to bridge the legacy
 *  /buy endpoint. RxDB manages `_deleted` (tombstone) + its own internal fields. */
// dev-lint: allow-wire-mirror RxDB owns the _deleted tombstone dimension;
// the wire type adds it in the replication layer, not in this local doc.
export interface ShoppingDoc {
  ulid: string;
  id: number | null;
  name: string;
  quantity: number | null;
  unit: string | null;
  barcode: string | null;
  /** Inventory category the buy→inventory conversion will use (ItemCategory
   *  string; the server validates it at push). */
  category: string;
  /** Optional link to the products catalog (mirrors items.product_id) — how a
   *  barcodeless shop product rides the Buy list. */
  product_id: number | null;
  done: boolean;
  rev: number;
}

const schema: RxJsonSchema<ShoppingDoc> = {
  // Bump the version + migrate on ANY schema change, else existing local DBs hit
  // a hash mismatch. v1: category + product_id (buy→inventory identity).
  version: 1,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    name: { type: 'string' },
    quantity: { type: ['number', 'null'] },
    unit: { type: ['string', 'null'] },
    barcode: { type: ['string', 'null'] },
    category: { type: 'string' },
    product_id: { type: ['integer', 'null'] },
    done: { type: 'boolean' },
    rev: { type: 'number' },
  },
  required: ['ulid', 'name', 'category', 'done', 'rev'],
};

// Exported for shopping-store.spec.ts — a stale local DB (an old browser
// profile, the Android WebView) runs these once on next open, so pin them.
export const migrationStrategies = {
  // v1: pre-identity rows take the same defaults the server backfill gives
  // them (0024: category 'food', no product link), so both sides converge
  // without a rev bump.
  1: (doc: Record<string, unknown>): Record<string, unknown> => ({
    ...doc,
    category: 'food',
    product_id: null,
  }),
};

/** What identifies "the same thing to buy" across catalogs. Matching tries the
 *  strongest key first: the catalog link, then the barcode, then the name
 *  (case-insensitive). Null keys never match null — two hand-typed rows are
 *  only the same thing if their names say so. */
export interface BuyIdentity {
  name: string;
  barcode: string | null;
  product_id: number | null;
}

export function matchesIdentity(doc: ShoppingDoc, identity: BuyIdentity): boolean {
  if (identity.product_id != null && doc.product_id === identity.product_id) return true;
  if (identity.barcode != null && doc.barcode === identity.barcode) return true;
  return doc.name.trim().toLowerCase() === identity.name.trim().toLowerCase();
}

/** The synced content fields (everything but the identity/server fields). */
type ShoppingContent = Omit<ShoppingDoc, 'ulid' | 'id' | 'rev'>;

/** Type-directed 3-way-merge spec: every content field with a strategy valid for
 *  its type (see [[makeConflictHandler]]). Exhaustive by construction. */
const SHOPPING_FIELDS: FieldSpec<ShoppingContent> = {
  name: 'value',
  quantity: 'value',
  unit: 'value',
  barcode: 'value',
  category: 'value',
  product_id: 'value',
  done: 'value',
};

/** The field-name allowlist the Conflicts screen may patch on "use other",
 *  derived from the spec so the two can never drift apart. */
export const SHOPPING_MERGE_FIELDS = Object.keys(SHOPPING_FIELDS) as (keyof ShoppingContent)[];

/** Local-first store for the shopping list — the machinery lives in
 *  {@link SyncedStore}; this declares only the collection and its content. */
@Injectable({ providedIn: 'root' })
export class ShoppingStore extends SyncedStore<ShoppingDoc> {
  private reporter = inject(ConflictReporter);

  /** Live, sorted, non-deleted shopping rows (unbought before bought). */
  readonly items$ = this.liveQuery([{ done: 'asc' }, { name: 'asc' }]);

  protected config(): SyncedCollectionConfig<ShoppingDoc> {
    return {
      name: 'shopping',
      schema,
      // Field-level 3-way merge: concurrent edits to different fields both
      // survive; same-field collisions keep this device's value and land in the
      // server-side conflict log for review.
      conflictHandler: makeConflictHandler<ShoppingDoc>({
        fields: SHOPPING_FIELDS,
        onConflicts: (kept, conflicts) =>
          this.reporter.report('shopping', kept.ulid, kept.name, conflicts),
      }),
      // '-v2': replication-state reset after the isEqual push-loss bug — see the
      // comment in wellbeing-store.ts.
      identifier: 'shopping-http-sync-v2',
      path: '/api/sync/shopping',
      label: 'shopping sync',
      trashKind: 'shopping',
      migrationStrategies,
    };
  }

  async add(input: {
    name: string;
    quantity: number | null;
    unit: string | null;
    barcode: string | null;
    category: string;
    product_id: number | null;
  }): Promise<void> {
    const col = await this.collection;
    await col.insert({
      ulid: ulid(),
      id: null,
      name: input.name,
      quantity: input.quantity,
      unit: input.unit,
      barcode: input.barcode,
      category: input.category,
      product_id: input.product_id,
      done: false,
      rev: 0,
    });
  }

  /** The un-done row for the same thing, if the list already has one — what the
   *  Inventory→Buy bridge checks before adding a duplicate. */
  async findActive(identity: BuyIdentity): Promise<ShoppingDoc | null> {
    const col = await this.collection;
    const docs = await col.find({ selector: { done: false } }).exec();
    return (
      docs.map((d) => d.toJSON() as ShoppingDoc).find((d) => matchesIdentity(d, identity)) ?? null
    );
  }

  async setDone(key: string, done: boolean): Promise<void> {
    await this.patch(key, { done });
  }

  /** Remove every ticked-off row (local; syncs as tombstones). */
  async clearDone(): Promise<void> {
    const col = await this.collection;
    await col.find({ selector: { done: true } }).remove();
  }
}
