import { Injectable } from '@angular/core';
import { ulid } from 'ulid';
import { type RxConflictHandler, type RxJsonSchema } from 'rxdb';

import { LinkKind, TargetKind } from '../models';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';

/** A to-do connection stored locally. `from` is the source to-do's ulid; the
 *  target is a soft ref (`targetRef` interpreted per `targetKind`). Mirrors the
 *  backend `TodoLinkDoc`. */
// dev-lint: allow-wire-mirror RxDB owns the _deleted tombstone dimension;
// the wire type adds it in the replication layer, not in this local doc.
export interface TodoLinkDoc {
  ulid: string;
  id: number | null;
  from: string;
  kind: LinkKind;
  targetKind: TargetKind;
  targetRef: string;
  rev: number;
}

const schema: RxJsonSchema<TodoLinkDoc> = {
  version: 0,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    from: { type: 'string', maxLength: 26 },
    kind: { type: 'string', enum: ['depends_on', 'subtask', 'related'], maxLength: 16 },
    targetKind: {
      type: 'string',
      enum: ['todo', 'item', 'recipe', 'room', 'shopping', 'place'],
      maxLength: 16,
    },
    targetRef: { type: 'string', maxLength: 255 },
    rev: { type: 'number' },
  },
  required: ['ulid', 'from', 'kind', 'targetKind', 'targetRef', 'rev'],
};

// Links are insert/delete-only — no editable fields — so this deliberately does
// NOT use the shared field-level `makeConflictHandler` (shopping/todo do). A
// tombstone stands; otherwise local wins. If links ever gain an editable field,
// switch to makeConflictHandler so one side's edit isn't silently dropped.
const conflictHandler: RxConflictHandler<TodoLinkDoc> = {
  isEqual: (a, b) => a.rev === b.rev && !!a._deleted === !!b._deleted,
  resolve: ({ realMasterState, newDocumentState }) =>
    Promise.resolve(realMasterState._deleted ? realMasterState : newDocumentState),
};

/** Local-first store for the to-do connection edges — the machinery lives in
 *  {@link SyncedStore}. Insert/delete-only (no content edits, no trash-restore
 *  undo), so it declares only its collection, custom conflict handler, and the
 *  add-with-dedup / bulk-remove operations. */
@Injectable({ providedIn: 'root' })
export class TodoLinkStore extends SyncedStore<TodoLinkDoc> {
  /** Live, non-deleted connection edges (natural order). */
  readonly links$ = this.liveQuery();

  protected config(): SyncedCollectionConfig<TodoLinkDoc> {
    return {
      name: 'todo_link',
      schema,
      conflictHandler,
      identifier: 'todo-link-http-sync',
      path: '/api/sync/todo-link',
      label: 'todo-link sync',
    };
  }

  async add(input: {
    from: string;
    kind: LinkKind;
    targetKind: TargetKind;
    targetRef: string;
  }): Promise<void> {
    const col = await this.collection;
    const dup = await col
      .findOne({
        selector: {
          from: input.from,
          kind: input.kind,
          targetKind: input.targetKind,
          targetRef: input.targetRef,
        },
      })
      .exec();
    if (dup) return;
    await col.insert({ ulid: ulid(), id: null, rev: 0, ...input });
  }

  /** Remove every edge touching a to-do (from OR target) — used when a to-do is
   *  deleted so it leaves no dangling connections. */
  async removeForTodo(todoUlid: string): Promise<void> {
    const col = await this.collection;
    await col
      .find({
        selector: {
          $or: [{ from: todoUlid }, { targetKind: 'todo', targetRef: todoUlid }],
        },
      })
      .remove();
  }
}
