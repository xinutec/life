import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { Shopping } from './shopping';

const doc = (over: Partial<ShoppingDoc>): ShoppingDoc => ({
  ulid: '01SPEC0000000000000000000A',
  id: 1,
  name: 'Milk',
  quantity: null,
  unit: null,
  barcode: null,
  category: 'food',
  product_id: null,
  done: false,
  rev: 1,
  ...over,
});

function setup(items: ShoppingDoc[], failIds: number[] = []) {
  const store = {
    items$: of(items),
    syncError: signal<string | null>(null),
    setDone: vi.fn(() => Promise.resolve()),
    remove: vi.fn(() => Promise.resolve()),
    revive: vi.fn(() => Promise.resolve()),
    reSync: vi.fn(),
    clearDone: vi.fn(() => Promise.resolve()),
  };
  const api = {
    buyShopping: vi.fn((id: number) =>
      failIds.includes(id) ? throwError(() => new Error('offline')) : of(void 0),
    ),
    restoreTrash: vi.fn(() => of(void 0)),
  };
  const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
  TestBed.configureTestingModule({
    providers: [
      Shopping,
      { provide: ShoppingStore, useValue: store },
      { provide: LifeApi, useValue: api },
      { provide: Feedback, useValue: feedback },
      { provide: MatBottomSheet, useValue: { open: vi.fn() } },
    ],
  });
  return { c: TestBed.inject(Shopping), store, api, feedback };
}

describe('Shopping buyDone', () => {
  it('converts every checked, synced row and summarises the win', () => {
    const { c, store, api, feedback } = setup([
      doc({ ulid: 'A'.repeat(26), id: 1, done: true }),
      doc({ ulid: 'B'.repeat(26), id: 2, done: true, name: 'Beans' }),
      doc({ ulid: 'C'.repeat(26), id: 3, done: false, name: 'Bread' }),
    ]);
    c.buyDone();
    expect(api.buyShopping).toHaveBeenCalledTimes(2);
    expect(store.remove).toHaveBeenCalledTimes(2);
    expect(feedback.notify).toHaveBeenCalledWith('2 added to inventory.');
    expect(feedback.error).not.toHaveBeenCalled();
  });

  it('keeps failed rows on the list and accounts for them honestly', () => {
    const { c, store, feedback } = setup(
      [
        doc({ ulid: 'A'.repeat(26), id: 1, done: true }),
        doc({ ulid: 'B'.repeat(26), id: 2, done: true, name: 'Beans' }),
      ],
      [2], // Beans fails server-side
    );
    c.buyDone();
    // Only the row the server actually inventoried is removed locally.
    expect(store.remove).toHaveBeenCalledExactlyOnceWith('A'.repeat(26));
    expect(feedback.error).toHaveBeenCalledWith('1 added to inventory; 1 failed and stayed on the list.');
    expect(feedback.notify).not.toHaveBeenCalled();
  });

  it('skips never-synced rows (no server id) and does nothing when none qualify', () => {
    const { c, api, feedback } = setup([doc({ id: null, done: true })]);
    c.buyDone();
    expect(api.buyShopping).not.toHaveBeenCalled();
    expect(feedback.notify).not.toHaveBeenCalled();
    expect(feedback.error).not.toHaveBeenCalled();
  });
});
