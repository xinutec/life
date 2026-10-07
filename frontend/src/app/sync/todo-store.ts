import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { TodoPriority, TodoStatus, TodoType } from '../models';
import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';
import { keysOf } from '../shared/narrow';

/** A to-do as stored locally; mirrors the backend `TodoDoc`. */
export interface TodoDoc {
  ulid: string;
  id: number | null;
  title: string;
  type: TodoType;
  status: TodoStatus;
  priority: TodoPriority | null;
  notes: string | null;
  /** YYYY-MM-DD before which it is "waiting". */
  notBefore: string | null;
  /** YYYY-MM-DD deadline. */
  due: string | null;
  /** Published to the case-file site; private unless chosen. */
  shared: boolean;
  rev: number;
}

const schema: RxJsonSchema<TodoDoc> = {
  // Any schema change needs a version bump and a migration.
  version: 4,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    title: { type: 'string' },
    type: {
      type: 'string',
      enum: ['purchase', 'call', 'appointment', 'admin', 'task'],
      maxLength: 16,
    },
    status: { type: 'string', enum: ['open', 'done'], maxLength: 8 },
    priority: { type: ['string', 'null'], maxLength: 8 },
    notes: { type: ['string', 'null'] },
    notBefore: { type: ['string', 'null'], maxLength: 10 },
    due: { type: ['string', 'null'], maxLength: 10 },
    shared: { type: 'boolean' },
    rev: { type: 'number' },
  },
  required: ['ulid', 'title', 'type', 'status', 'rev'],
};

type TodoContent = Omit<TodoDoc, 'ulid' | 'id' | 'rev'>;

const TODO_FIELDS: FieldSpec<TodoContent> = {
  title: 'value',
  type: 'value',
  status: 'value',
  priority: 'value',
  notes: 'value',
  notBefore: 'value',
  due: 'value',
  shared: 'value',
};

/** The fields "use other" may patch on the Conflicts screen. */
export const TODO_MERGE_FIELDS = keysOf(TODO_FIELDS);

/** The to-do list. */
@Injectable({ providedIn: 'root' })
export class TodoStore extends SyncedStore<TodoDoc> {
  private reporter = inject(ConflictReporter);

  /** Open before done, then by title. */
  readonly items$ = this.liveQuery([{ status: 'desc' }, { title: 'asc' }]);

  protected config(): SyncedCollectionConfig<TodoDoc> {
    return {
      name: 'todo',
      schema,
      conflictHandler: makeConflictHandler<TodoDoc>({
        fields: TODO_FIELDS,
        onConflicts: (kept, conflicts) =>
          this.reporter.report('todo', kept.ulid, kept.title, conflicts),
      }),
      identifier: 'todo-http-sync-v2',
      path: '/api/sync/todo',
      label: 'todo sync',
      trashKind: 'todo',
      migrationStrategies: {
        1: (doc: Record<string, unknown>) => doc,
        2: (doc: Record<string, unknown>) => ({ ...doc, priority: doc['priority'] ?? null }),
        3: (doc: Record<string, unknown>) => ({
          ...doc,
          notBefore: doc['notBefore'] ?? null,
          due: doc['due'] ?? null,
        }),
        4: (doc: Record<string, unknown>) => ({ ...doc, shared: doc['shared'] ?? false }),
      },
    };
  }

  async add(input: {
    title: string;
    type: TodoType;
    priority: TodoPriority | null;
    notes: string | null;
    notBefore?: string | null;
    due?: string | null;
  }): Promise<void> {
    const col = await this.collection;
    await col.insert({
      ulid: ulid(),
      id: null,
      title: input.title,
      type: input.type,
      status: 'open',
      priority: input.priority,
      notes: input.notes,
      notBefore: input.notBefore ?? null,
      due: input.due ?? null,
      shared: false,
      rev: 0,
    });
  }

  async setStatus(key: string, status: TodoStatus): Promise<void> {
    await this.patch(key, { status });
  }
}
