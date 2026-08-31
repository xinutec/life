import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { WellbeingEntry } from '../wellbeing/wellbeing-entry';
import { LifeApi } from '../../life-api';
import { BinDay } from '../../models';
import { ShoppingStore } from '../../sync/shopping-store';
import { TodoStore } from '../../sync/todo-store';
import { TodoGraph } from '../todo/todo-graph';
import { Today } from './today';

const todo = (ulid: string, over: Record<string, unknown> = {}) => ({
  ulid,
  id: null,
  title: ulid,
  type: 'task',
  status: 'open',
  priority: null,
  notes: null,
  notBefore: null,
  due: null,
  rev: 0,
  ...over,
});

describe('Today', () => {
  function setup(opts: {
    todos?: ReturnType<typeof todo>[];
    state?: Record<string, string>;
    urgency?: Record<string, string>;
    items?: unknown[];
    shopping?: { done: boolean }[];
    bins?: BinDay[];
  }) {
    const todos = opts.todos ?? [];
    const api = {
      items: vi.fn(() => of(opts.items ?? [])),
      bins: vi.fn(() => of(opts.bins ?? [])),
    };
    const shopping = { items$: of(opts.shopping ?? []) };
    const todoStore = { items$: of(todos), setStatus: vi.fn().mockResolvedValue(undefined) };
    const sheet = { open: vi.fn() };
    const graph = {
      todoItems: () => todos,
      statusOf: vi.fn((t: { ulid: string }) => opts.state?.[t.ulid] ?? 'open'),
      urgencyOf: vi.fn((t: { ulid: string }) => opts.urgency?.[t.ulid] ?? 'none'),
      daysUntil: vi.fn(() => -2),
    };
    TestBed.configureTestingModule({
      providers: [
        Today,
        { provide: LifeApi, useValue: api },
        { provide: ShoppingStore, useValue: shopping },
        { provide: TodoStore, useValue: todoStore },
        { provide: TodoGraph, useValue: graph },
        { provide: MatBottomSheet, useValue: sheet },
        { provide: Feedback, useValue: { undo: vi.fn() } },
      ],
    });
    return { c: TestBed.inject(Today), todoStore, sheet };
  }

  it('surfaces overdue and ready to-dos, hiding blocked/waiting/done/plain-open', () => {
    const { c: t } = setup({
      todos: [
        todo('overdue', { due: '2026-01-01' }),
        todo('ready'),
        todo('blocked'),
        todo('waiting'),
        todo('plain'),
        todo('done', { status: 'done' }),
      ],
      state: { overdue: 'open', ready: 'ready', blocked: 'blocked', waiting: 'waiting', plain: 'open', done: 'done' },
      urgency: { overdue: 'overdue' },
    });
    const shown = t.attention().map((a) => a.todo.ulid);
    expect(shown).toContain('overdue');
    expect(shown).toContain('ready');
    expect(shown).not.toContain('blocked');
    expect(shown).not.toContain('waiting');
    expect(shown).not.toContain('plain'); // open but not urgent or ready
    expect(shown).not.toContain('done');
    // Overdue sorts above ready.
    expect(shown[0]).toBe('overdue');
  });

  it('lists only expired/soon items, soonest first', () => {
    const { c: t } = setup({
      items: [
        { id: 1, name: 'Old milk', expiry: '2026-01-01' }, // expired
        { id: 2, name: 'Fresh', expiry: '2099-01-01' }, // ok → hidden
      ],
    });
    const names = t.expiring().map((e) => e.item.name);
    expect(names).toEqual(['Old milk']);
  });

  it('counts unbought shopping items', () => {
    const { c: t } = setup({ shopping: [{ done: false }, { done: false }, { done: true }] });
    expect(t.buyCount()).toBe(2);
  });

  it('ticking a row completes the to-do and offers Undo back to open', () => {
    const { c: t, todoStore } = setup({});
    const feedback = TestBed.inject(Feedback) as unknown as { undo: ReturnType<typeof vi.fn> };
    t.complete(todo('a') as never);
    expect(todoStore.setStatus).toHaveBeenCalledWith('a', 'done');
    const undoFn = (feedback.undo.mock.calls[0] as [string, () => void])[1];
    undoFn();
    expect(todoStore.setStatus).toHaveBeenCalledWith('a', 'open');
  });

  it('opens the just-logged check-in, rather than sending you to another screen', () => {
    // Today carries the same strip, so it had the same defect. It was left out
    // of the first fix on the reasoning that Today has no edit sheet — wrong, it
    // opens one for to-dos. Checking in from the landing screen must not be the
    // worse flow.
    const { c, sheet } = setup({});
    c.addDetail('01ABCDEF0000000000000000AB');
    expect(sheet.open).toHaveBeenCalledWith(WellbeingEntry, {
      data: { ulid: '01ABCDEF0000000000000000AB' },
    });
  });
});
