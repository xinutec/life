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
    rev: 1,
    ...overrides,
  };
}

function setup(doc = makeDoc()) {
  // A writable signal standing in for the live RxDB-backed list, so tests can
  // simulate a remote edit landing while the sheet is open.
  const items = signal<TodoDoc[]>([doc]);
  const graph = {
    todoItems: items,
    refreshCatalogs: vi.fn(),
    statusOf: () => 'open' as const,
    blockers: () => [],
    outgoing: () => [],
    incoming: () => [],
    search: () => [],
    add: vi.fn(),
    removeLink: vi.fn(),
    removeLinksForTodo: vi.fn(),
  };
  const store = {
    patch: vi.fn(() => Promise.resolve()),
    setStatus: vi.fn(() => Promise.resolve()),
    remove: vi.fn(() => Promise.resolve()),
    revive: vi.fn(() => Promise.resolve()),
    reSync: vi.fn(),
  };
  TestBed.configureTestingModule({
    imports: [TodoDetail],
    providers: [
      { provide: TodoGraph, useValue: graph },
      { provide: TodoStore, useValue: store },
      { provide: LifeApi, useValue: { restoreTrash: vi.fn() } },
      { provide: Feedback, useValue: { undo: vi.fn() } },
      { provide: MatBottomSheetRef, useValue: { dismiss: vi.fn() } },
      { provide: MatBottomSheet, useValue: { open: vi.fn() } },
      { provide: MAT_BOTTOM_SHEET_DATA, useValue: { ulid: doc.ulid } },
    ],
  });
  const fixture = TestBed.createComponent(TodoDetail);
  fixture.detectChanges();
  return { fixture, cmp: fixture.componentInstance, store, items };
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
