import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { HttpErrorResponse } from '@angular/common/http';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { CoverageQuery } from '../../models';
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

function setup(
  items: ShoppingDoc[],
  failIds: number[] = [],
  coverage: { key: string; sources: string[] }[] = [],
) {
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
    shopCoverage: vi.fn((rows: CoverageQuery[]) =>
      of(coverage.filter((c) => rows.some((r) => r.key === c.key))),
    ),
  };
  const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
  // buyDone now asks where the shopping happened before it buys anything, so a
  // bare `open` mock would leave every buy test waiting on a sheet that never
  // answers. `dismissed` is what the sheet handed back: 'skip' is the
  // buy-without-prices path these tests were written against.
  let dismissed: unknown = 'skip';
  const setDismissed = (v: unknown) => {
    dismissed = v;
  };
  const sheet = { open: vi.fn(() => ({ afterDismissed: () => of(dismissed) })) };
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
  return { c: TestBed.inject(Shopping), store, api, feedback, sheet, router, setDismissed };
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

  it('records a price against the row it was typed for, and nothing for the rest', () => {
    // The prices map is keyed by row id: a price typed for one thing must not
    // ride along with another, and an empty box is not a price of zero.
    const { c, api, setDismissed } = setup([
      doc({ ulid: 'A'.repeat(26), id: 1, done: true }),
      doc({ ulid: 'B'.repeat(26), id: 2, done: true, name: 'Beans' }),
    ]);
    setDismissed({ shop: 'Waitrose', prices: new Map([[2, 330]]) });
    c.buyDone();
    expect(api.buyShopping).toHaveBeenCalledWith(1, undefined);
    expect(api.buyShopping).toHaveBeenCalledWith(2, { shop: 'Waitrose', amount_minor: 330 });
  });

  it('buys nothing when the sheet is dismissed without choosing', () => {
    // Closing a sheet opened by mistake must not empty the list — the buy is
    // irreversible from the list's point of view.
    const { c, api, setDismissed } = setup([doc({ ulid: 'A'.repeat(26), id: 1, done: true })]);
    setDismissed(undefined);
    c.buyDone();
    expect(api.buyShopping).not.toHaveBeenCalled();
  });

  it('skips never-synced rows (no server id) and does nothing when none qualify', () => {
    const { c, api, feedback } = setup([doc({ id: null, done: true })]);
    c.buyDone();
    expect(api.buyShopping).not.toHaveBeenCalled();
    expect(feedback.notify).not.toHaveBeenCalled();
    expect(feedback.error).not.toHaveBeenCalled();
  });
});

/** Where each row is sold, from remembered lookups — never a stock check. */
describe('Shopping shop coverage', () => {
  const linked = (over: Partial<ShoppingDoc>) => doc({ product_id: 42, ...over });

  it('asks only about rows there is something to ask about', () => {
    // A ticked-off row is already in the trolley, and a free-text jotting has no
    // identity to look up — asking about either would be noise the shops' memory
    // cannot answer.
    const { c, api } = setup([
      linked({ ulid: 'a' }),
      doc({ ulid: 'b', product_id: null, barcode: null }),
      linked({ ulid: 'c', done: true }),
      doc({ ulid: 'd', product_id: null, barcode: '5000000000123' }),
    ]);
    TestBed.tick();
    expect(c.items().length).toBe(4);
    const asked = api.shopCoverage.mock.calls[0][0];
    expect(asked.map((r) => r.key)).toEqual(['a', 'd']);
  });

  it('names the shops on the row that has them, and nothing on the row that has none', () => {
    const { c } = setup([linked({ ulid: 'a' }), linked({ ulid: 'b' })], [], [
      { key: 'a', sources: ['asda', 'waitrose'] },
      { key: 'b', sources: [] },
    ]);
    TestBed.tick();
    expect(c.shopLine(linked({ ulid: 'a' }))).toBe('Asda · Waitrose');
    expect(c.shopsFor(linked({ ulid: 'b' }))).toEqual([]);
  });

  it('counts the trip per shop, best first, and says what it could not answer', () => {
    const { c } = setup(
      [
        linked({ ulid: 'a' }),
        linked({ ulid: 'b' }),
        doc({ ulid: 'c', product_id: null, barcode: null }), // unanswerable
      ],
      [],
      [
        { key: 'a', sources: ['asda', 'waitrose'] },
        { key: 'b', sources: ['asda'] },
      ],
    );
    TestBed.tick();
    const trip = c.tripSummary()!;
    expect(trip.shops).toEqual([
      { label: 'Asda', have: 2 },
      { label: 'Waitrose', have: 1 },
    ]);
    // The denominator is what we asked about; the jotting is named separately
    // rather than counted as a shop's failure.
    expect(trip.of).toBe(2);
    expect(trip.unknown).toBe(1);
  });

  it('a ticked-off row leaves the trip it is no longer part of', () => {
    const { c } = setup([linked({ ulid: 'a' }), linked({ ulid: 'b', done: true })], [], [
      { key: 'a', sources: ['asda'] },
    ]);
    TestBed.tick();
    expect(c.tripSummary()!.of).toBe(1);
  });

  it('says it could not check, rather than showing an empty list as an answer', () => {
    // Offline, "no shops known" and "we could not ask" look identical unless the
    // screen distinguishes them — and one of them would send you shopping blind.
    const store = {
      items$: of([linked({ ulid: 'a' })]),
      syncError: signal<string | null>(null),
      setDone: vi.fn(),
      remove: vi.fn(),
      revive: vi.fn(),
      reSync: vi.fn(),
      clearDone: vi.fn(),
    };
    const api = {
      shopCoverage: vi.fn(() => throwError(() => new HttpErrorResponse({ status: 0 }))),
    };
    TestBed.configureTestingModule({
      providers: [
        Shopping,
        { provide: ShoppingStore, useValue: store },
        { provide: LifeApi, useValue: api },
        { provide: Feedback, useValue: { notify: vi.fn(), error: vi.fn(), undo: vi.fn() } },
        { provide: MatBottomSheet, useValue: { open: vi.fn() } },
        { provide: Router, useValue: { navigate: vi.fn() } },
      ],
    });
    const c = TestBed.inject(Shopping);
    TestBed.tick();
    expect(c.coverageOffline()).toBe(true);
    expect(c.tripSummary()).toBeNull();
  });
});
