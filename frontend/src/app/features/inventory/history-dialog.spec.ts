import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { HttpErrorResponse } from '@angular/common/http';
import { of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import { Item, ItemHistoryEntry, Purchase } from '../../models';
import { HistoryDialog, HistoryDialogData } from './history-dialog';

const DAY = 86_400_000;

const item: Item = {
  id: 7,
  product_id: null,
  name: 'Greek Yoghurt',
  brand: null,
  category: 'food',
  quantity: 750,
  unit: 'g',
  expiry: null,
  expiry_precision: 'day',
  location_id: null,
  barcode: null,
  has_image: false,
};

function setup(
  opts: { entries?: ItemHistoryEntry[]; purchases?: Purchase[]; error?: unknown } = {},
) {
  const itemHistory = vi.fn(() =>
    opts.error !== undefined
      ? throwError(() => opts.error)
      : of({ entries: opts.entries ?? [], purchases: opts.purchases ?? [] }),
  );
  TestBed.configureTestingModule({
    imports: [HistoryDialog],
    providers: [
      { provide: MatDialogRef, useValue: { close: vi.fn() } },
      { provide: MAT_DIALOG_DATA, useValue: { item } satisfies HistoryDialogData },
      { provide: LifeApi, useValue: { itemHistory } },
    ],
  });
  const fixture = TestBed.createComponent(HistoryDialog);
  fixture.detectChanges();
  return { fixture, cmp: fixture.componentInstance, itemHistory };
}

/** A purchase as the server sends it. Named so the two tests below read as
 *  being about what is SHOWN, not about assembling a fixture. */
function purchase(over: Partial<Purchase> = {}): Purchase {
  return {
    id: 1,
    item_id: 7,
    product_id: null,
    barcode: null,
    name: 'Milk',
    shop: 'Waitrose',
    amount_minor: 250,
    currency: 'GBP',
    quantity: 2,
    unit: 'l',
    unit_amount_minor: 125,
    unit_measure: 'L',
    bought_at: new Date().toISOString(),
    warranty_months: null,
    warranty_until: null,
    ...over,
  };
}

function text(fixture: { nativeElement: unknown }): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('HistoryDialog', () => {
  it('says how much WENT for a use, and how much was on hand for anything else', () => {
    // The one real trap in this data: `quantity` is a delta on a `used` row and
    // a level on every other one. The number alone cannot say which, so the
    // words have to — "Used 200g" against "200g on hand".
    const { cmp } = setup({
      entries: [
        { id: 3, event: 'used', quantity: 200, location: 'Fridge', at: Date.now() },
        { id: 1, event: 'added', quantity: 950, location: 'Cupboard', at: Date.now() - 3 * DAY },
      ],
    });
    expect(cmp.lines()[0].title).toBe('Used 200 g');
    expect(cmp.lines()[0].detail).toBe('Fridge');
    expect(cmp.lines()[1].title).toBe('Added');
    expect(cmp.lines()[1].detail).toBe('950 g on hand · Cupboard');
  });

  it('a move says where to', () => {
    // "Moved" on its own is the version of this that is not worth reading.
    const { cmp } = setup({
      entries: [{ id: 2, event: 'moved', quantity: null, location: 'Fridge', at: Date.now() }],
    });
    expect(cmp.lines()[0].title).toBe('Moved');
    expect(cmp.lines()[0].detail).toBe('to Fridge');
  });

  it('keeps the order the server gave it', () => {
    // Newest first is decided by the SQL (`ORDER BY at DESC, id DESC`), which is
    // the only place that can break the tie between two events in one second.
    // Re-sorting here would be a second opinion that could disagree with it.
    const { cmp } = setup({
      entries: [
        { id: 9, event: 'used', quantity: 1, location: null, at: 5 },
        { id: 4, event: 'moved', quantity: null, location: null, at: 5 },
        { id: 2, event: 'added', quantity: 3, location: null, at: 5 },
      ],
    });
    expect(cmp.lines().map((l) => l.id)).toEqual([9, 4, 2]);
  });

  it('an item with no history says why rather than looking broken', () => {
    const { fixture } = setup({ entries: [] });
    expect(text(fixture)).toContain('added before the app kept a history');
  });

  it('offline says the history is a server thing, not that there is none', () => {
    // An empty list and a failed load look identical if you only render one of
    // them, and they mean opposite things about what happened to the item.
    const { fixture, cmp } = setup({ error: new HttpErrorResponse({ status: 0 }) });
    fixture.detectChanges();
    expect(cmp.error()).toContain('No connection');
    expect(text(fixture)).not.toContain('added before the app kept a history');
  });

  it('retry asks again', () => {
    const { cmp, itemHistory } = setup({ entries: [] });
    cmp.load();
    expect(itemHistory).toHaveBeenCalledTimes(2);
    expect(itemHistory).toHaveBeenCalledWith(7);
  });
});

describe('HistoryDialog purchases', () => {
  it('shows what was paid, with the rate, for a row that has no product at all', () => {
    // The whole reason this lives here: a hand-typed buy-list row has no barcode
    // and no catalogue product, so the product page cannot show its price. The
    // item is the only key that always exists.
    const { fixture } = setup({ purchases: [purchase()] });
    const t = text(fixture);
    expect(t).toContain('£2.50');
    expect(t).toContain('£1.25/L');
    expect(t).toContain('Waitrose');
  });

  it('does not read as empty when the only thing recorded is a price', () => {
    // The empty state says "it was added before the app kept a history", which
    // directly above a price would be a contradiction.
    const { fixture } = setup({ entries: [], purchases: [purchase()] });
    expect(text(fixture)).not.toContain('Nothing recorded');
  });
});
