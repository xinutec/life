import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';
import { keysOf } from '../shared/narrow';

/** A Buy row as stored locally; `id` is used only by /buy. */
// dev-lint: allow-wire-mirror RxDB owns the _deleted tombstone dimension;
// the wire type adds it in the replication layer, not in this local doc.
export interface ShoppingDoc {
  ulid: string;
  id: number | null;
  name: string;
  quantity: number | null;
  unit: string | null;
  barcode: string | null;
  /** The category the bought item gets; validated by the server at push. */
  category: string;
  product_id: number | null;
  done: boolean;
  rev: number;
}

const schema: RxJsonSchema<ShoppingDoc> = {
  // Any schema change needs a version bump and a migration.
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

// Exported so the spec can pin them: an old device runs them on next open.
export const migrationStrategies = {
  // The defaults migration 0024 gives the same rows server-side.
  1: (doc: Record<string, unknown>): Record<string, unknown> => ({
    ...doc,
    category: 'food',
    product_id: null,
  }),
};

/** "The same thing to buy": by catalog link, then barcode, then name. A null
 *  key never matches a null. */
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

/** What a caller decides about a new Buy row. */
export type BuyInput = Omit<ShoppingDoc, 'ulid' | 'id' | 'done' | 'rev'>;

function newRow(input: BuyInput): ShoppingDoc {
  return { ulid: ulid(), id: null, ...input, done: false, rev: 0 };
}

/** Which of `inputs` the list hasn't got yet. Each is also matched against the
 *  ones accepted before it: a thing named twice is one row. */
export function planAdditions(
  active: readonly ShoppingDoc[],
  inputs: readonly BuyInput[],
): { fresh: BuyInput[]; already: string[] } {
  const fresh: BuyInput[] = [];
  const already: string[] = [];
  const staged: ShoppingDoc[] = [];
  for (const input of inputs) {
    const identity = { name: input.name, barcode: input.barcode, product_id: input.product_id };
    if ([...active, ...staged].some((d) => matchesIdentity(d, identity))) {
      already.push(input.name);
      continue;
    }
    fresh.push(input);
    staged.push({ ulid: '', id: null, ...input, done: false, rev: 0 });
  }
  return { fresh, already };
}

type ShoppingContent = Omit<ShoppingDoc, 'ulid' | 'id' | 'rev'>;

const SHOPPING_FIELDS: FieldSpec<ShoppingContent> = {
  name: 'value',
  quantity: 'value',
  unit: 'value',
  barcode: 'value',
  category: 'value',
  product_id: 'value',
  done: 'value',
};

/** The fields "use other" may patch on the Conflicts screen. */
export const SHOPPING_MERGE_FIELDS = keysOf(SHOPPING_FIELDS);

/** The Buy list. */
@Injectable({ providedIn: 'root' })
export class ShoppingStore extends SyncedStore<ShoppingDoc> {
  private reporter = inject(ConflictReporter);

  /** Unbought before bought. */
  readonly items$ = this.liveQuery([{ done: 'asc' }, { name: 'asc' }]);

  protected config(): SyncedCollectionConfig<ShoppingDoc> {
    return {
      name: 'shopping',
      schema,
      conflictHandler: makeConflictHandler<ShoppingDoc>({
        fields: SHOPPING_FIELDS,
        onConflicts: (kept, conflicts) =>
          this.reporter.report('shopping', kept.ulid, kept.name, conflicts),
      }),
      identifier: 'shopping-http-sync-v2',
      path: '/api/sync/shopping',
      label: 'shopping sync',
      trashKind: 'shopping',
      migrationStrategies,
    };
  }

  async add(input: BuyInput): Promise<void> {
    const col = await this.collection;
    await col.insert(newRow(input));
  }

  /** Add what the list lacks, and say which were already there. */
  async addMissing(inputs: readonly BuyInput[]): Promise<{ added: string[]; already: string[] }> {
    const col = await this.collection;
    const docs = await col.find({ selector: { done: false } }).exec();
    const { fresh, already } = planAdditions(
      docs.map((d) => d.toJSON() as ShoppingDoc),
      inputs,
    );
    if (fresh.length > 0) await col.bulkInsert(fresh.map(newRow));
    return { added: fresh.map((f) => f.name), already };
  }

  /** The un-done row for the same thing, if any. */
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

  /** Remove every ticked-off row. */
  async clearDone(): Promise<void> {
    const col = await this.collection;
    await col.find({ selector: { done: true } }).remove();
  }
}
