import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import {
  MAT_BOTTOM_SHEET_DATA,
  MatBottomSheet,
  MatBottomSheetRef,
} from '@angular/material/bottom-sheet';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { TodoDoc, TodoStore } from '../../sync/todo-store';
import { TodoDetail } from './todo-detail';
import { TodoGraph } from './todo-graph';

function makeDoc(overrides: Partial<TodoDoc> = {}): TodoDoc {
  return {
    ulid: '01TEST0000000000000000TODO',
    id: 1,
    title: 'Original title',
    type: 'task',
    status: 'open',
    priority: null,
    notes: 'original notes',
    notBefore: null,
    due: null,
    shared: false,
    rev: 1,
    ...overrides,
  };
}

interface SetupOpts {
  state?: 'open' | 'ready' | 'blocked' | 'waiting' | 'done';
  outgoing?: unknown[];
  incoming?: unknown[];
}

function setup(doc = makeDoc(), opts: SetupOpts = {}) {
  // A writable signal standing in for the live RxDB-backed list, so tests can
  // simulate a remote edit landing while the sheet is open.
  const items = signal<TodoDoc[]>([doc]);
  const graph = {
    todoItems: items,
    refreshCatalogs: vi.fn(),
    statusOf: () => opts.state ?? ('open' as const),
    blockers: () => [],
    outgoing: () => opts.outgoing ?? [],
    incoming: () => opts.incoming ?? [],
    search: () => [],
    add: vi.fn(),
    removeLink: vi.fn(),
    removeLinksForTodo: vi.fn(),
  };
  const store = {
    patch: vi.fn(() => Promise.resolve()),
    setStatus: vi.fn(() => Promise.resolve()),
    remove: vi.fn(() => Promise.resolve()),
    undoDelete: vi.fn(() => Promise.resolve()),
  };
  const feedback = { undo: vi.fn() };
  TestBed.configureTestingModule({
    imports: [TodoDetail],
    providers: [
      { provide: TodoGraph, useValue: graph },
      { provide: TodoStore, useValue: store },
      { provide: LifeApi, useValue: { restoreTrash: vi.fn() } },
      { provide: Feedback, useValue: feedback },
      { provide: MatBottomSheetRef, useValue: { dismiss: vi.fn() } },
      { provide: MatBottomSheet, useValue: { open: vi.fn() } },
      { provide: MAT_BOTTOM_SHEET_DATA, useValue: { ulid: doc.ulid } },
    ],
  });
  const fixture = TestBed.createComponent(TodoDetail);
  fixture.detectChanges();
  return { fixture, cmp: fixture.componentInstance, store, items, graph, feedback };
}

describe('TodoDetail dismiss-flush', () => {
  it('does not write the seeded values back over a remote edit (untouched fields)', () => {
    const { fixture, store, items } = setup();
    // Another device edits while the sheet is open; the user types nothing.
    items.set([makeDoc({ title: 'Remote title', notes: 'remote notes', rev: 2 })]);
    fixture.destroy();
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('flushes a typed-but-unblurred edit on dismiss — only the dirty field', () => {
    const { fixture, cmp, store } = setup();
    cmp.onTitleInput('New title');
    fixture.destroy();
    expect(store.patch).toHaveBeenCalledExactlyOnceWith(makeDoc().ulid, { title: 'New title' });
  });

  it('does not re-flush an edit already saved by blur', () => {
    const { fixture, cmp, store } = setup();
    cmp.onNotesInput('typed notes');
    cmp.saveNotes(); // blur
    store.patch.mockClear();
    fixture.destroy();
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('does not flush anything when the to-do is being deleted', () => {
    const { fixture, cmp, store } = setup();
    cmp.onTitleInput('typed while deleting');
    cmp.remove();
    store.patch.mockClear();
    fixture.destroy();
    expect(store.patch).not.toHaveBeenCalled();
  });
});

describe('TodoDetail groups', () => {
  const edge = (linkKind: string, ref: string, label: string) => ({
    ulid: `01EDGE${ref.padEnd(20, '0')}`,
    linkKind,
    target: { kind: 'todo', ref, label },
    source: { kind: 'todo', ref, label },
  });

  it('resolves connections into headed groups, hiding empty ones', () => {
    const { cmp } = setup(makeDoc(), {
      outgoing: [edge('depends_on', 'DEP1', 'Buy paint'), edge('subtask', 'SUB1', 'Sand the door')],
      incoming: [edge('related', 'REL1', 'Decorate hallway')],
    });
    expect(cmp.groups().map((g) => g.heading)).toEqual(['Depends on', 'Subtasks', 'Related']);
    const related = cmp.groups().find((g) => g.heading === 'Related')!;
    expect(related.rows[0].target.label).toBe('Decorate hallway');
  });

  it('shows nothing when unconnected', () => {
    expect(setup().cmp.groups()).toEqual([]);
  });
});

describe('TodoDetail guards', () => {
  it('refuses to complete a blocked to-do', () => {
    const { cmp, store } = setup(makeDoc(), { state: 'blocked' });
    cmp.toggleDone();
    expect(store.setStatus).not.toHaveBeenCalled();
  });

  it('always allows un-completing a done one', () => {
    const { cmp, store } = setup(makeDoc({ status: 'done' }), { state: 'blocked' });
    cmp.toggleDone();
    expect(store.setStatus).toHaveBeenCalledWith(makeDoc().ulid, 'open');
  });
});

describe('TodoDetail delete undo', () => {
  it('defers link removal to the Undo window close, so undo keeps connections', () => {
    const { cmp, graph, feedback } = setup();
    cmp.remove();
    // Links are still intact while the Undo window is open…
    expect(graph.removeLinksForTodo).not.toHaveBeenCalled();
    // …and are removed only when the window closes un-undone.
    const onClose = feedback.undo.mock.calls[0][2] as () => void;
    onClose();
    expect(graph.removeLinksForTodo).toHaveBeenCalledExactlyOnceWith(makeDoc().ulid);
  });
});
