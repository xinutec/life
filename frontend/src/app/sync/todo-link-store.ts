import { Injectable } from '@angular/core';
import { ulid } from 'ulid';
import { type RxConflictHandler, type RxJsonSchema } from 'rxdb';

import { LinkKind, TargetKind } from '../models';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';

/** A to-do connection; `targetRef` is read per `targetKind`. Mirrors the backend
 *  `TodoLinkDoc`. */
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

// Links have no editable fields, so a tombstone stands and otherwise local wins.
// An editable field would need makeConflictHandler.
const conflictHandler: RxConflictHandler<TodoLinkDoc> = {
  isEqual: (a, b) => a.rev === b.rev && !!a._deleted === !!b._deleted,
  resolve: ({ realMasterState, newDocumentState }) =>
    Promise.resolve(realMasterState._deleted ? realMasterState : newDocumentState),
};

/** The to-do connections: insert and delete only. */
@Injectable({ providedIn: 'root' })
export class TodoLinkStore extends SyncedStore<TodoLinkDoc> {
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

  /** Remove every edge from or to a to-do. */
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
