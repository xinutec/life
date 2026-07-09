import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { TodoPriority, TodoStatus, TodoType } from '../models';
import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';

/** A to-do row as stored locally. `ulid` is the stable identity; `rev` is the
 *  last server revision seen (set by sync, not local edits); `id` is the server
 *  autoincrement (null until synced). RxDB manages `_deleted` + internal fields.
 *  Mirrors the backend `TodoDoc` wire shape. */
export interface TodoDoc {
  ulid: string;
  id: number | null;
  title: string;
  type: TodoType;
  status: TodoStatus;
  priority: TodoPriority | null;
  notes: string | null;
  /** Start-gate (YYYY-MM-DD): can't act before this day → "waiting". */
  notBefore: string | null;
  /** Deadline (YYYY-MM-DD): drives urgency ordering. */
  due: string | null;
  rev: number;
}

const schema: RxJsonSchema<TodoDoc> = {
  // Bump the version + add a migration on ANY schema change, else existing local
  // DBs hit a hash mismatch. v1: `type` enum widened. v2: `priority` added.
  // v3: `notBefore` + `due` timing added.
  version: 3,
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
    rev: { type: 'number' },
  },
  required: ['ulid', 'title', 'type', 'status', 'rev'],
};

/** The synced content fields (everything but the identity/server fields). */
type TodoContent = Omit<TodoDoc, 'ulid' | 'id' | 'rev'>;

/** Type-directed 3-way-merge spec: every content field with a strategy valid for
 *  its type (see [[makeConflictHandler]]). Exhaustive by construction — a field
 *  added to TodoDoc won't compile until it's classified here. */
const TODO_FIELDS: FieldSpec<TodoContent> = {
  title: 'value',
  type: 'value',
  status: 'value',
  priority: 'value',
  notes: 'value',
  notBefore: 'value',
  due: 'value',
};

/** The field-name allowlist the Conflicts screen may patch on "use other",
 *  derived from the spec so the two can never drift apart. */
export const TODO_MERGE_FIELDS = Object.keys(TODO_FIELDS) as (keyof TodoContent)[];

/** Local-first store for the to-do list — the machinery lives in
 *  {@link SyncedStore}; this declares only the collection and its content. */
@Injectable({ providedIn: 'root' })
export class TodoStore extends SyncedStore<TodoDoc> {
  private reporter = inject(ConflictReporter);

  /** Live, sorted, non-deleted to-dos: open before done, then by title. */
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
      // '-v2': replication-state reset after the isEqual push-loss bug — see the
      // comment in wellbeing-store.ts.
      identifier: 'todo-http-sync-v2',
      path: '/api/sync/todo',
      label: 'todo sync',
      trashKind: 'todo',
      migrationStrategies: {
        1: (doc: Record<string, unknown>) => doc, // enum widened; existing docs already valid
        2: (doc: Record<string, unknown>) => ({ ...doc, priority: doc['priority'] ?? null }), // add priority field
        3: (doc: Record<string, unknown>) => ({
          ...doc,
          notBefore: doc['notBefore'] ?? null,
          due: doc['due'] ?? null,
        }), // add timing fields
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
      rev: 0,
    });
  }

  async setStatus(key: string, status: TodoStatus): Promise<void> {
    await this.patch(key, { status });
  }
}
