import { Injectable, inject, signal } from '@angular/core';
import { Observable, from, fromEvent, merge, throwError, timer } from 'rxjs';
import { map, retry, shareReplay, switchMap, take } from 'rxjs/operators';
import {
  type MangoQuerySortPart,
  type MigrationStrategies,
  type RxCollection,
  type RxConflictHandler,
  type RxJsonSchema,
} from 'rxdb';

import { LifeApi } from '../life-api';
import { TrashKind } from '../models';
import { isNotFound, onlineHint } from '../shared/api-error';
import { Feedback } from '../shared/feedback';
import { AuthState } from './auth-state';
import { LifeDb } from './life-db';
import { startHttpReplication } from './replication';
import { SyncSource, SyncStatus } from './sync-status';

/** An undo's server restore is retried this often before it is reported. */
const RESTORE_ATTEMPTS = 20;

/** The identity and server fields every synced document carries; the rest is
 *  content. */
export interface SyncDoc {
  ulid: string;
  /** Null until the row has synced. */
  id: number | null;
  /** Last server revision seen; set by sync, never by a local edit. */
  rev: number;
}

/** What a concrete store declares about its collection. */
export interface SyncedCollectionConfig<T> {
  name: string;
  schema: RxJsonSchema<T>;
  conflictHandler: RxConflictHandler<T>;
  /** Bump to make RxDB re-examine every document. */
  identifier: string;
  path: string;
  label: SyncSource;
  /** Needed for {@link undoDelete} to restore on the server. */
  trashKind?: TrashKind;
  migrationStrategies?: MigrationStrategies;
}

/** Base of every local-first synced store: an RxDB collection, its replication,
 *  and patch / remove / undo. `collection` starts on a microtask so `config()`
 *  runs after the subclass's field initialisers. */
@Injectable()
export abstract class SyncedStore<T extends SyncDoc> {
  /** A sync problem to show, or null. */
  readonly syncError = signal<string | null>(null);

  private lifeDb = inject(LifeDb);
  private syncStatus = inject(SyncStatus);
  private api = inject(LifeApi);
  private auth = inject(AuthState);
  private feedback = inject(Feedback);
  private replication?: ReturnType<typeof startHttpReplication<T>>;
  private cfg!: SyncedCollectionConfig<T>;

  protected abstract config(): SyncedCollectionConfig<T>;

  protected readonly collection: Promise<RxCollection<T>> = Promise.resolve().then(() =>
    this.init(),
  );

  /** The live, non-deleted rows; primary-key order without a sort. */
  protected liveQuery(sort?: MangoQuerySortPart<T>[]): Observable<T[]> {
    return from(this.collection).pipe(
      switchMap((col) => (sort ? col.find({ sort }) : col.find()).$),
      // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- RxDocumentData<T> is T plus RxDB's own fields
      map((docs) => docs.map((d) => d.toJSON() as T)),
      shareReplay({ bufferSize: 1, refCount: false }),
    );
  }

  /** A local edit of content fields; identity and server fields are not editable. */
  async patch(key: string, fields: Partial<Omit<T, keyof SyncDoc>>): Promise<void> {
    const doc = await this.find(key);
    // eslint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- content keys are a subset of keyof T
    await doc?.incrementalPatch(fields as Partial<T>);
  }

  /** Tombstone one row. */
  async remove(key: string): Promise<void> {
    const doc = await this.find(key);
    await doc?.remove();
  }

  /** Bring a removed doc back locally, under the same ulid. */
  async revive(doc: T): Promise<void> {
    const col = await this.collection;
    await col.insert({ ...doc });
  }

  /** Undo a delete: revive locally, and restore on the server, since no push
   *  can clear a server tombstone. A 404 means the delete never got there. Any
   *  other failure is retried, as the revived row would otherwise vanish at the
   *  next pull. */
  async undoDelete(doc: T): Promise<void> {
    await this.revive(doc);
    const kind = this.cfg.trashKind;
    if (!kind || doc.id == null) return;
    this.api
      .restoreTrash(kind, doc.ulid)
      .pipe(
        retry({
          count: RESTORE_ATTEMPTS - 1,
          delay: (e: unknown) =>
            isNotFound(e)
              ? throwError(() => e)
              : merge(fromEvent(window, 'online'), timer(this.restoreRetryMs)).pipe(take(1)),
        }),
      )
      .subscribe({
        next: () => this.reSync(),
        error: (e: unknown) => {
          if (isNotFound(e)) return;
          this.feedback.error(
            `Could not undo the delete${onlineHint(e)} — restore it from Recently deleted.`,
          );
        },
      });
  }

  /** Overridden by tests. */
  protected restoreRetryMs = 30_000;

  /** Pull now. `start()`, not RxDB's `reSync()`, which a tab that lost the
   *  leader election silently drops. */
  reSync(): void {
    void this.replication?.start();
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
      onAuthLost: () => this.auth.lose(),
    });
    return col;
  }
}
