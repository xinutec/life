import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatDialog } from '@angular/material/dialog';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { Shops } from '../../shop';
import { ItemSheet, ItemSheetData } from './item-sheet';

const flush = () => new Promise((r) => setTimeout(r));

function setup(
  opts: {
    available?: boolean;
    candidate?: { external_id: string; name: string; image_url: string } | null;
    data?: ItemSheetData;
  } = {},
) {
  const dialog = {
    open: vi.fn(() => ({ afterClosed: () => of(opts.candidate ?? null) })),
  };
  const shops = {
    available: opts.available ?? false,
    connect: vi.fn().mockResolvedValue(undefined),
    search: vi.fn().mockResolvedValue([]),
    fetchProduct: vi.fn().mockResolvedValue({
      source: 'waitrose',
      external_id: '062593',
      name: 'Waitrose Cheddar',
      brand: 'Waitrose',
      barcodes: ['5000169005125'],
      image_url: 'https://cdn/img.jpg',
      display_price: null,
      categories: [],
    }),
  };
  const api = {
    createItem: vi.fn(() => of({ id: 7 })),
    updateItem: vi.fn(() => of({ id: 7 })),
    lookupProduct: vi.fn(() => of({})),
    importProduct: vi.fn(() => of({ id: 99, name: 'Waitrose Cheddar' })),
  };
  const ref = { dismiss: vi.fn() };
  const feedback = { notify: vi.fn(), error: vi.fn() };

  TestBed.configureTestingModule({
    imports: [ItemSheet],
    providers: [
      { provide: MatDialog, useValue: dialog },
      { provide: Shops, useValue: shops },
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
  return { cmp: fixture.componentInstance, dialog, shops, api, ref, feedback };
}

describe('ItemSheet shop enrichment', () => {
  it('offers no shop providers outside the app (no native bridge)', () => {
    expect(setup({ available: false }).cmp.shopProviders).toHaveLength(0);
  });

  it('offers providers inside the app', () => {
    const { cmp } = setup({ available: true });
    expect(cmp.shopProviders.map((p) => p.id)).toEqual(['waitrose']);
  });

  it('connectShop signs in and reports success', async () => {
    const { cmp, shops, feedback } = setup({ available: true });
    cmp.connectShop(cmp.shopProviders[0]);
    await flush();
    expect(shops.connect).toHaveBeenCalledOnce();
    expect(feedback.notify).toHaveBeenCalledWith(expect.stringContaining('Connected'));
  });

  it('finds → fetches → imports → links the item to the product', async () => {
    const { cmp, shops, api, feedback } = setup({
      available: true,
      candidate: { external_id: '062593', name: 'Cheddar', image_url: 'x' },
    });
    cmp.findOnShop(cmp.shopProviders[0]);
    await flush();

    expect(shops.fetchProduct).toHaveBeenCalledWith(cmp.shopProviders[0], '062593');
    expect(api.importProduct).toHaveBeenCalledWith(
      expect.objectContaining({ source: 'waitrose', external_id: '062593' }),
    );
    // The item is now linked to the imported catalog product.
    expect(cmp.form().product_id).toBe(99);
    expect(cmp.form().name).toBe('Waitrose Cheddar');
    expect(feedback.notify).toHaveBeenCalledWith(expect.stringContaining('Linked'));
  });

  it('does nothing when the picker is cancelled', async () => {
    const { cmp, shops, api } = setup({ available: true, candidate: null });
    cmp.findOnShop(cmp.shopProviders[0]);
    await flush();
    expect(shops.fetchProduct).not.toHaveBeenCalled();
    expect(api.importProduct).not.toHaveBeenCalled();
  });

  it('sends product_id on save so the link persists', () => {
    const { cmp, api } = setup({ available: true });
    cmp.patch({ name: 'Cheddar', product_id: 99 });
    cmp.save();
    expect(api.createItem).toHaveBeenCalledWith(expect.objectContaining({ product_id: 99 }));
  });
});
