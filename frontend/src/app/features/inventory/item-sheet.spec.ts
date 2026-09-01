import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatDialog } from '@angular/material/dialog';
import { Observable, of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { ProductPick } from '../../shared/product-picker';
import { LifeApi } from '../../life-api';
import { ItemSheet, ItemSheetData } from './item-sheet';

const flush = () => new Promise((r) => setTimeout(r));

function setup(
  opts: {
    pick?: ProductPick | null;
    data?: ItemSheetData;
  } = {},
) {
  const dialog = {
    open: vi.fn(() => ({ afterClosed: () => of(opts.pick ?? null) })),
  };
  const api = {
    // Declared with its argument type, so a test can read the body that was sent
    // without reaching through an `any`. Absence of a key is what two of these
    // tests check, and `expect.anything()` cannot express that without one.
    createItem: vi.fn<(body: Record<string, unknown>) => Observable<{ id: number }>>(() =>
      of({ id: 7 }),
    ),
    updateItem: vi.fn(() => of({ id: 7 })),
    lookupProduct: vi.fn(() => of({})),
  };
  const ref = { dismiss: vi.fn() };
  const feedback = { notify: vi.fn(), error: vi.fn() };

  TestBed.configureTestingModule({
    imports: [ItemSheet],
    providers: [
      { provide: MatDialog, useValue: dialog },
      { provide: LifeApi, useValue: api },
      { provide: MatBottomSheetRef, useValue: ref },
      { provide: Feedback, useValue: feedback },
      { provide: MAT_BOTTOM_SHEET_DATA, useValue: opts.data ?? { locations: [] } },
    ],
  });
  // The sheet imports MatDialogModule, which re-provides the real MatDialog at
  // the component injector — overrideProvider forces our stub at every level.
  TestBed.overrideProvider(MatDialog, { useValue: dialog });
  const fixture = TestBed.createComponent(ItemSheet);
  fixture.detectChanges();
  return { cmp: fixture.componentInstance, dialog, api, ref, feedback };
}

describe('ItemSheet product linking', () => {
  it('a pick fills name/barcode/link, and unit only when empty', async () => {
    const { cmp } = setup({
      pick: {
        name: 'Waitrose Cheddar',
        barcode: '5000169005125',
        product_id: 99,
        quantity: null,
        unit: 'g',
        category: 'food',
      },
    });
    cmp.findProduct();
    await flush();
    expect(cmp.form().name).toBe('Waitrose Cheddar');
    expect(cmp.form().barcode).toBe('5000169005125');
    expect(cmp.form().product_id).toBe(99);
    expect(cmp.form().unit).toBe('g');
  });

  it('a pick never overwrites a unit the user already chose', async () => {
    const { cmp } = setup({
      pick: {
        name: 'Cheddar',
        barcode: null,
        product_id: 99,
        quantity: null,
        unit: 'g',
        category: null,
      },
    });
    cmp.patch({ unit: 'block' });
    cmp.findProduct();
    await flush();
    expect(cmp.form().unit).toBe('block');
  });

  it('a pack size fills the quantity, so a linked row starts out measured', async () => {
    // 75 of 84 rows in the cupboard are linked to a product and 3 carry a
    // quantity: the label held the number all along and had no way to hand it
    // over. This is that way.
    const { cmp } = setup({
      pick: {
        name: 'Greek Yoghurt',
        barcode: null,
        product_id: 7,
        quantity: 950,
        unit: 'g',
        category: null,
      },
    });
    cmp.findProduct();
    await flush();
    expect(cmp.form().quantity).toBe(950);
    expect(cmp.form().unit).toBe('g');
  });

  it('a pick never overwrites a quantity the user already typed', async () => {
    // A number already in the box is a measurement of THIS row — half a tub —
    // and the pack size is only what it held when it was new.
    const { cmp } = setup({
      pick: {
        name: 'Greek Yoghurt',
        barcode: null,
        product_id: 7,
        quantity: 950,
        unit: 'g',
        category: null,
      },
    });
    cmp.patch({ quantity: 400 });
    cmp.findProduct();
    await flush();
    expect(cmp.form().quantity).toBe(400);
  });

  it('does nothing when the picker is cancelled', async () => {
    const { cmp } = setup({ pick: null });
    cmp.patch({ name: 'typed' });
    cmp.findProduct();
    await flush();
    expect(cmp.form().name).toBe('typed');
    expect(cmp.form().product_id).toBeNull();
  });

  it('sends product_id on save so the link persists', () => {
    const { cmp, api } = setup();
    cmp.patch({ name: 'Cheddar', product_id: 99 });
    cmp.save();
    expect(api.createItem).toHaveBeenCalledWith(expect.objectContaining({ product_id: 99 }));
  });

  // Only this form sees the keystroke, so only this form can say whose name it
  // is. The server sees a name and a linked product and cannot tell them apart —
  // guessing there froze scribbles as intentions and stripped real overrides.
  it('claims the name only when it was typed by hand', () => {
    const { cmp, api } = setup();
    cmp.renameByHand('Oregano');
    cmp.save();
    expect(api.createItem).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Oregano', name_source: 'user' }),
    );
  });

  it('says nothing about the name when it came from a product pick', async () => {
    // The pick supplies the catalogue's own name, so the item must keep
    // following the product — a correction should still reach it later.
    const { cmp, api } = setup({
      pick: {
        name: 'Waitrose Cheddar',
        barcode: null,
        product_id: 7,
        quantity: null,
        unit: null,
        category: null,
      },
    });
    cmp.renameByHand('chedar');
    cmp.findProduct();
    await flush();
    cmp.save();
    expect(api.createItem).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Waitrose Cheddar' }),
    );
    expect('name_source' in api.createItem.mock.calls[0][0]).toBe(false);
  });

  it('says nothing on a plain save, so an existing choice is left alone', () => {
    // Opening an item and saving it unchanged must not make a claim either way:
    // `name_source` absent tells the server to preserve what the item had.
    const { cmp, api } = setup();
    cmp.patch({ name: 'Cheddar' });
    cmp.save();
    expect('name_source' in api.createItem.mock.calls[0][0]).toBe(false);
  });
});

/**
 * A medicine box carries MM/YYYY and nothing else. The DATE column needs a day,
 * so the form's job is to never make somebody choose one that is not printed —
 * and to say which of the two it asked for, because the server cannot tell.
 */
describe('ItemSheet expiry precision', () => {
  it('asks a medication for a month, and everything else for a date', () => {
    const { cmp } = setup();
    expect(cmp.form().expiry_precision).toBe('day');
    cmp.chooseCategory('medication');
    expect(cmp.form().expiry_precision).toBe('month');
    cmp.chooseCategory('food');
    expect(cmp.form().expiry_precision).toBe('day');
  });

  it('leaves a date that has already been typed alone', () => {
    // Re-categorising is not a statement about the date. Widening a full date to
    // a month would throw away a day somebody read off a jar.
    const { cmp } = setup();
    cmp.patch({ expiry: '2026-09-14' });
    cmp.chooseCategory('medication');
    expect(cmp.form().expiry_precision).toBe('day');
    expect(cmp.form().expiry).toBe('2026-09-14');
  });

  it('stores a picked month as its LAST day', () => {
    // 06/2028 is good THROUGH June. The 1st would expire it twenty-nine days
    // early, which for a prescription is the expensive direction to be wrong.
    const { cmp } = setup();
    cmp.setPrecision('month');
    cmp.setExpiryMonth('2028-06');
    expect(cmp.form().expiry).toBe('2028-06-30');
    expect(cmp.expiryMonth()).toBe('2028-06');
  });

  it('keeps the date across a change of precision, rather than dropping it', () => {
    const { cmp } = setup();
    cmp.patch({ expiry: '2028-06-14' });
    cmp.setPrecision('month');
    expect(cmp.form().expiry).toBe('2028-06-30');
    cmp.setPrecision('day');
    expect(cmp.form().expiry).toBe('2028-06-30');
  });

  it('sends the precision, because the server cannot infer it', () => {
    // The stored 30th is identical whether it was printed or invented. Only this
    // form saw which question was answered.
    const { cmp, api } = setup();
    cmp.patch({ name: 'Tablets' });
    cmp.setPrecision('month');
    cmp.setExpiryMonth('2028-06');
    cmp.save();
    expect(api.createItem).toHaveBeenCalledWith(
      expect.objectContaining({ expiry: '2028-06-30', expiry_precision: 'month' }),
    );
  });
});
