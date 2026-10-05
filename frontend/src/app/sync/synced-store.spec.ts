import { HttpErrorResponse } from '@angular/common/http';
import { Injectable } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Observable, defer, of, throwError } from 'rxjs';
import { createRxDatabase, type RxConflictHandler, type RxJsonSchema } from 'rxdb';
import { getRxStorageMemory } from 'rxdb/plugins/storage-memory';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../life-api';
import { Feedback } from '../shared/feedback';
import { makeConflictHandler } from './conflict-merge';
import { LifeDb } from './life-db';
import { SyncDoc, SyncedCollectionConfig, SyncedStore } from './synced-store';

/** Undo of a synced delete. The server keeps a tombstone that only a trash
 *  restore clears, so a restore that fails for any reason but "never deleted
 *  there" must be retried, and reported if it never lands. */

interface Doc extends SyncDoc {
  name: string;
}

const schema: RxJsonSchema<Doc> = {
  version: 0,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    name: { type: 'string' },
    rev: { type: 'number' },
  },
  required: ['ulid', 'name', 'rev'],
};

@Injectable()
class TestStore extends SyncedStore<Doc> {
  protected override restoreRetryMs = 5;
  protected config(): SyncedCollectionConfig<Doc> {
    return {
      name: 'entries',
      schema,
      conflictHandler: makeConflictHandler<Doc>({
        fields: { name: 'value' },
        onConflicts: () => undefined,
      }),
      identifier: 'synced-store-spec',
      path: '/api/sync/test',
      label: 'shopping sync',
      trashKind: 'shopping',
    };
  }
}

let seq = 0;

/** A fresh in-memory database per test, in place of the shared IndexedDB one. */
const memoryDb = {
  async collection(name: string, s: RxJsonSchema<Doc>, conflictHandler: RxConflictHandler<Doc>) {
    // ast-grep-ignore: life-single-rxdb
    const db = await createRxDatabase({
      name: `synced-store-spec-${++seq}`,
      storage: getRxStorageMemory(),
      multiInstance: false,
    });
    const added = await db.addCollections({ [name]: { schema: s, conflictHandler } });
    return added[name];
  },
};

const status = (code: number) => new HttpErrorResponse({ status: code });

/** `attempts` counts subscriptions, not calls: a retry re-subscribes to the
 *  one cold request, as HttpClient's is, and each subscription is a request. */
function setup(answers: (() => Observable<void>)[]) {
  const attempts = vi.fn();
  const restoreTrash = vi.fn(() =>
    defer(() => {
      attempts();
      return answers[Math.min(attempts.mock.calls.length - 1, answers.length - 1)]();
    }),
  );
  const error = vi.fn();
  TestBed.configureTestingModule({
    providers: [
      TestStore,
      { provide: LifeDb, useValue: memoryDb },
      { provide: LifeApi, useValue: { restoreTrash } },
      { provide: Feedback, useValue: { error } },
    ],
  });
  return { store: TestBed.inject(TestStore), restoreTrash, attempts, error };
}

const synced: Doc = { ulid: '01JTESTULID0000000000000001', id: 7, name: 'Milk', rev: 3 };

describe('SyncedStore.undoDelete', () => {
  // Replication polls the server; nothing here is about the pull, so it hangs.
  beforeEach(() =>
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => undefined)),
    ),
  );
  afterEach(() => vi.unstubAllGlobals());

  it('retries a restore that failed for a reason other than not-found', async () => {
    const { store, attempts, error } = setup([
      () => throwError(() => status(0)),
      () => throwError(() => status(503)),
      () => of(undefined),
    ]);
    await store.undoDelete(synced);
    await vi.waitFor(() => expect(attempts).toHaveBeenCalledTimes(3));
    expect(error).not.toHaveBeenCalled();
  });

  it('stops at not-found: the delete never reached the server', async () => {
    const { store, attempts, error } = setup([() => throwError(() => status(404))]);
    await store.undoDelete(synced);
    await new Promise((r) => setTimeout(r, 50));
    expect(attempts).toHaveBeenCalledTimes(1);
    expect(error).not.toHaveBeenCalled();
  });

  it('says so when the restore never lands', async () => {
    const { store, attempts, error } = setup([() => throwError(() => status(503))]);
    await store.undoDelete(synced);
    await vi.waitFor(() => expect(error).toHaveBeenCalledTimes(1));
    expect(attempts).toHaveBeenCalledTimes(20);
    expect(error.mock.calls[0][0]).toContain('Recently deleted');
  });

  it('asks the server nothing for a row it never had', async () => {
    const { store, restoreTrash } = setup([() => of(undefined)]);
    await store.undoDelete({ ...synced, id: null });
    expect(restoreTrash).not.toHaveBeenCalled();
  });
});
