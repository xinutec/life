import { Injectable, inject, signal } from '@angular/core';
import { Observable, from } from 'rxjs';
import { map, shareReplay, switchMap } from 'rxjs/operators';
import {
  type MangoQuerySortPart,
  type MigrationStrategies,
  type RxCollection,
  type RxConflictHandler,
  type RxJsonSchema,
} from 'rxdb';

import { LifeApi } from '../life-api';
import { TrashKind } from '../models';
import { LifeDb } from './life-db';
import { startHttpReplication } from './replication';
import { SyncStatus } from './sync-status';

/** The identity + server-managed fields every synced document carries. A
 *  collection's editable *content* is `Omit<T, keyof SyncDoc>`. */
export interface SyncDoc {
  /** Stable client identity (ULID); the RxDB primary key. */
  ulid: string;
  /** Server autoincrement — null until the row has synced; used only to bridge
   *  legacy id-keyed endpoints and to know whether a delete reached the server. */
  id: number | null;
  /** Last server revision seen (set by sync, never by a local edit). */
  rev: number;
}

/** Everything collection-specific a concrete store declares; the base supplies
 *  the rest of the machinery (local DB wiring, replication, the reactive query,
 *  patch / remove / two-layer undo). */
export interface SyncedCollectionConfig<T> {
  /** RxDB collection name on the shared `lifedb`. */
  name: string;
  schema: RxJsonSchema<T>;
  /** How a pull-time merge resolves: field-level for editable collections, a
   *  set-only tombstone for insert/delete-only ones. */
  conflictHandler: RxConflictHandler<T>;
  /** Stable RxDB replication identity (bump to force a full re-examine). */
  identifier: string;
  /** Sync endpoint, e.g. '/api/sync/shopping'. */
  path: string;
  /** console.warn tag + sync-status source key, e.g. 'shopping sync'. */
  label: string;
  /** Trash table this collection restores from — required to offer a restorable
   *  delete (see {@link undoDelete}); omit for insert/delete-only collections
   *  that never surface an Undo. */
  trashKind?: TrashKind;
  /** RxDB schema migrations, if the schema has changed version. */
  migrationStrategies?: MigrationStrategies;
}

/** The shared spine of every local-first synced store: one on-device RxDB
 *  collection as the source of truth, background HTTP replication, and the
 *  reactive/offline read + optimistic write grammar. A concrete store supplies
 *  its {@link SyncedCollectionConfig} via {@link config} and exposes a typed
 *  reactive list through {@link liveQuery}; the patch / remove / revive / undo
 *  mechanics are inherited so they can't drift between collections — the same
 *  discipline B6 imposed on the server half.
 *
 *  The `collection` promise is initialised on a microtask (`Promise.resolve()
 *  .then(...)`) so `config()` is read AFTER every subclass field initialiser has
 *  run — a config that references injected subclass state (e.g. the
 *  ConflictReporter) is fully wired by the time it's evaluated. */
@Injectable()
export abstract class SyncedStore<T extends SyncDoc> {
  /** null = ok; a string = a sync problem to surface (e.g. login required). */
  readonly syncError = signal<string | null>(null);

  private lifeDb = inject(LifeDb);
  private syncStatus = inject(SyncStatus);
  private api = inject(LifeApi);
  private replication?: ReturnType<typeof startHttpReplication<T>>;
  private cfg!: SyncedCollectionConfig<T>;

  /** The concrete store's one-time, collection-specific description. */
  protected abstract config(): SyncedCollectionConfig<T>;

  protected readonly collection: Promise<RxCollection<T>> = Promise.resolve().then(() =>
    this.init(),
  );

  /** A live, offline, reactive view of the non-deleted rows (RxDB filters
   *  tombstones). Pass a sort; omit it for natural (primary-key) order. */
  protected liveQuery(sort?: MangoQuerySortPart<T>[]): Observable<T[]> {
    return from(this.collection).pipe(
      switchMap((col) => (sort ? col.find({ sort }) : col.find()).$),
      map((docs) => docs.map((d) => d.toJSON() as T)),
      shareReplay({ bufferSize: 1, refCount: false }),
    );
  }

  /** Optimistic local edit of one row's content fields (never the identity /
   *  server fields — hence the content-only parameter type). */
  async patch(key: string, fields: Partial<Omit<T, keyof SyncDoc>>): Promise<void> {
    const doc = await this.find(key);
    // Content keys are a subset of `keyof T`, so the widening cast is sound; it
    // just bridges the generic `T` to RxDB's own patch type.
    await doc?.incrementalPatch(fields as Partial<T>);
  }

  /** Soft-delete (tombstone) one row; syncs as a set-only tombstone. */
  async remove(key: string): Promise<void> {
    const doc = await this.find(key);
    await doc?.remove();
  }

  /** Bring a just-removed doc back locally under the same ulid (insert after
   *  remove revives the RxDB tombstone). Offline-safe; the local half of
   *  {@link undoDelete}. */
  async revive(doc: T): Promise<void> {
    const col = await this.collection;
    await col.insert({ ...doc });
  }

  /** Undo a delete both layers deep: revive locally (works offline) AND, for a
   *  row the server has already seen, the authoritative server-side trash
   *  restore. A plain re-push can never clear a server tombstone (the set-only
   *  rule), so a synced row's undo MUST go through the trash endpoint; a 404
   *  there just means our delete push hadn't landed yet and the local revive
   *  already covers it. */
  async undoDelete(doc: T): Promise<void> {
    await this.revive(doc);
    if (this.cfg.trashKind && doc.id != null) {
      this.api.restoreTrash(this.cfg.trashKind, doc.ulid).subscribe({
        next: () => this.reSync(),
        error: () => {}, // 404 = delete push never arrived; the revive covers it
      });
    }
  }

  /** Ask replication to pull now — e.g. right after a server-side trash restore,
   *  so a resurrected row appears without waiting for the next natural sync. */
  reSync(): void {
    this.replication?.reSync();
  }

  protected async find(key: string) {
    const col = await this.collection;
    return col.findOne(key).exec();
  }

  private async init(): Promise<RxCollection<T>> {
    this.cfg = this.config();
    const col = await this.lifeDb.collection(
      this.cfg.name,
      this.cfg.schema,
      this.cfg.conflictHandler,
      this.cfg.migrationStrategies,
    );
    this.replication = startHttpReplication<T>({
      collection: col,
      identifier: this.cfg.identifier,
      path: this.cfg.path,
      syncError: this.syncError,
      syncStatus: this.syncStatus,
      label: this.cfg.label,
    });
    return col;
  }
}
