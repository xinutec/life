import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { HttpErrorResponse } from '@angular/common/http';
import { Router } from '@angular/router';
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
    lookupProduct: vi.fn((barcode: string) => of({ id: 900, barcode })),
  };
  const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
  const sheet = { open: vi.fn() };
  const router = { navigate: vi.fn(() => Promise.resolve(true)) };
  TestBed.configureTestingModule({
    providers: [
      Shopping,
      { provide: ShoppingStore, useValue: store },
      { provide: LifeApi, useValue: api },
      { provide: Feedback, useValue: feedback },
      { provide: MatBottomSheet, useValue: sheet },
      { provide: Router, useValue: router },
    ],
  });
  return { c: TestBed.inject(Shopping), store, api, feedback, sheet, router };
}

describe('Shopping row tap → detail', () => {
  it('opens the product page directly for a linked row', () => {
    const { c, router, api } = setup([doc({ product_id: 42 })]);
    c.view(doc({ product_id: 42 }));
    expect(router.navigate).toHaveBeenCalledWith(['/product', 42]);
    expect(api.lookupProduct).not.toHaveBeenCalled(); // no lookup needed
  });

  it('resolves a barcode-only row to its product first', () => {
    const { c, router, api } = setup([]);
    c.view(doc({ product_id: null, barcode: '5000000000123' }));
    expect(api.lookupProduct).toHaveBeenCalledWith('5000000000123');
    expect(router.navigate).toHaveBeenCalledWith(['/product', 900]);
  });

  it('falls back to editing a free-text row that has no product to show', () => {
    const { c, router, sheet } = setup([]);
    c.view(doc({ product_id: null, barcode: null }));
    expect(router.navigate).not.toHaveBeenCalled();
    expect(sheet.open).toHaveBeenCalled(); // the edit sheet
  });

  it('reports an honest miss when a barcode resolves to nothing', () => {
    const { c, router, feedback, api } = setup([]);
    api.lookupProduct.mockReturnValueOnce(
      throwError(() => new HttpErrorResponse({ status: 404 })),
    );
    c.view(doc({ product_id: null, barcode: '5000000000999' }));
    expect(router.navigate).not.toHaveBeenCalled();
    expect(feedback.error).toHaveBeenCalledWith('No product found for 5000000000999.');
  });
});

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
