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

/** The one shared RxDB database; stores add their collections on demand.
 *  Creating `lifedb` twice throws in production, and one screen may use
 *  several stores, so `addCollections` calls are serialised here. */
@Injectable({ providedIn: 'root' })
export class LifeDb {
  private dbPromise?: Promise<RxDatabase>;
  private chain: Promise<unknown> = Promise.resolve();

  private db(): Promise<RxDatabase> {
    this.dbPromise ??= (async () => {
      if (isDevMode()) {
        // Imported statically it would SHIP: a dev-mode plugin in production is
        // worse than the split it avoids.
        // dev-lint: allow-dynamic-import a dev-mode plugin must not reach production
        const { RxDBDevModePlugin } = await import('rxdb/plugins/dev-mode');
        addRxPlugin(RxDBDevModePlugin);
      }
      // Schema migrations (e.g. the todo `type` enum widening) run at collection
      // add-time, so the plugin must be registered in prod too, not just dev —
      // which is why it is imported statically. It ships either way; loading it
      // dynamically only moved it into a second request at first DB use.
      addRxPlugin(RxDBMigrationSchemaPlugin);
      // THE single place the shared 'lifedb' is created; every store goes
      // through this service's collection(). Exempt from the singleton rule:
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

  /** Add (once) and return a named collection on the shared database. Calls are
   *  serialised so concurrent `collection()` calls can't race `addCollections`. */
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
        [name]: { schema, conflictHandler, ...(migrationStrategies ? { migrationStrategies } : {}) },
      });
      return added[name] as RxCollection<T>;
    });
    // Keep the chain alive even if this add fails, so later adds still run.
    this.chain = result.catch(() => undefined);
    return result;
  }
}
