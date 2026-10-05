import { Injectable, isDevMode } from '@angular/core';
import {
  addRxPlugin,
  createRxDatabase,
  type MigrationStrategies,
  type RxCollection,
  type RxConflictHandler,
  type RxDatabase,
  type RxJsonSchema,
} from 'rxdb';
import { RxDBMigrationSchemaPlugin } from 'rxdb/plugins/migration-schema';
import { getRxStorageDexie } from 'rxdb/plugins/storage-dexie';

/** The one shared RxDB database. Collections are added one at a time, since
 *  concurrent `addCollections` calls race. */
@Injectable({ providedIn: 'root' })
export class LifeDb {
  private dbPromise?: Promise<RxDatabase>;
  private chain: Promise<unknown> = Promise.resolve();

  private db(): Promise<RxDatabase> {
    this.dbPromise ??= (async () => {
      if (isDevMode()) {
        // dev-lint: allow-dynamic-import a dev-mode plugin must not reach production
        const { RxDBDevModePlugin } = await import('rxdb/plugins/dev-mode');
        addRxPlugin(RxDBDevModePlugin);
      }
      // Migrations run in production too.
      addRxPlugin(RxDBMigrationSchemaPlugin);
      // ast-grep-ignore: life-single-rxdb
      return createRxDatabase({
        name: 'lifedb',
        storage: getRxStorageDexie(),
        multiInstance: true,
        ignoreDuplicate: isDevMode(),
      });
    })();
    return this.dbPromise;
  }

  /** Add (once) and return a named collection. */
  collection<T>(
    name: string,
    schema: RxJsonSchema<T>,
    conflictHandler: RxConflictHandler<T>,
    migrationStrategies?: MigrationStrategies,
  ): Promise<RxCollection<T>> {
    const result = this.chain.then(async () => {
      const db = await this.db();
      const existing = db.collections[name] as RxCollection<T> | undefined;
      if (existing) return existing;
      const added = await db.addCollections({
        [name]: {
          schema,
          conflictHandler,
          ...(migrationStrategies ? { migrationStrategies } : {}),
        },
      });
      return added[name] as RxCollection<T>;
    });
    // dev-lint: allow-ignored-error a failed add must not block the ones after it; `result` still carries the error
    this.chain = result.catch(() => undefined);
    return result;
  }
}
