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
  done: boolean;
  rev: number;
}

const schema: RxJsonSchema<ShoppingDoc> = {
  version: 0,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    name: { type: 'string' },
    quantity: { type: ['number', 'null'] },
    unit: { type: ['string', 'null'] },
    barcode: { type: ['string', 'null'] },
    done: { type: 'boolean' },
    rev: { type: 'number' },
  },
  required: ['ulid', 'name', 'done', 'rev'],
};

/** The synced content fields (everything but the identity/server fields). */
type ShoppingContent = Omit<ShoppingDoc, 'ulid' | 'id' | 'rev'>;

/** Type-directed 3-way-merge spec: every content field with a strategy valid for
 *  its type (see [[makeConflictHandler]]). Exhaustive by construction. */
const SHOPPING_FIELDS: FieldSpec<ShoppingContent> = {
  name: 'value',
  quantity: 'value',
  unit: 'value',
  barcode: 'value',
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
    };
  }

  async add(input: {
    name: string;
    quantity: number | null;
    unit: string | null;
    barcode: string | null;
  }): Promise<void> {
    const col = await this.collection;
    await col.insert({
      ulid: ulid(),
      id: null,
      name: input.name,
      quantity: input.quantity,
      unit: input.unit,
      barcode: input.barcode,
      done: false,
      rev: 0,
    });
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
