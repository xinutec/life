import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatDialog } from '@angular/material/dialog';
import { of } from 'rxjs';
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
    createItem: vi.fn(() => of({ id: 7 })),
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
      pick: { name: 'Cheddar', barcode: null, product_id: 99, unit: 'g', category: null },
    });
    cmp.patch({ unit: 'block' });
    cmp.findProduct();
    await flush();
    expect(cmp.form().unit).toBe('block');
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
});
