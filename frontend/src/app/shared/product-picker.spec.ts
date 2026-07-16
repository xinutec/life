import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { signal } from '@angular/core';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from './feedback';
import { LifeApi } from '../life-api';
import { Item, Product } from '../models';
import { Shops } from '../shop';
import { ItemsStore } from '../stores/catalog';
import { ProductPicker, localHits, withoutLocalDupes } from './product-picker';

const item = (over: Partial<Item>): Item => ({
  id: 1,
  product_id: null,
  name: 'Yoghurt',
  brand: null,
  category: 'food',
  quantity: null,
  unit: null,
  expiry: null,
  location_id: null,
  barcode: null,
  has_image: false,
  ...over,
});

const product = (over: Partial<Product>): Product => ({
  id: 10,
  barcode: null,
  name: 'Yoghurt',
  brand: null,
  quantity_label: null,
  source: 'off',
  external_id: null,
  has_image: false,
  ...over,
});

describe('localHits', () => {
  it('matches name and brand substrings, case-insensitively', () => {
    const items = [
      item({ id: 1, name: 'Greek Yoghurt' }),
      item({ id: 2, name: 'Lassi', brand: 'Yoghurt Co' }),
      item({ id: 3, name: 'Oat milk' }),
    ];
    expect(localHits(items, 'yoghurt').map((i) => i.id)).toEqual([1, 2]);
  });

  it('ranks name-prefix hits before mid-word hits', () => {
    const items = [
      item({ id: 1, name: 'Greek Yoghurt' }),
      item({ id: 2, name: 'Yoghurt, natural' }),
    ];
    expect(localHits(items, 'yog').map((i) => i.id)).toEqual([2, 1]);
  });

  it('an empty query hits nothing (the list, not the world)', () => {
    expect(localHits([item({})], '  ')).toEqual([]);
  });
});

describe('withoutLocalDupes', () => {
  it('drops catalog rows already shown as a local item, by id or barcode', () => {
    const locals = [
      item({ id: 1, product_id: 10 }),
      item({ id: 2, barcode: '5000000000001' }),
    ];
    const catalog = [
      product({ id: 10 }), // dup by catalog id
      product({ id: 11, barcode: '5000000000001' }), // dup by barcode
      product({ id: 12, name: 'Something else' }),
    ];
    expect(withoutLocalDupes(catalog, locals).map((p) => p.id)).toEqual([12]);
  });

  it('two barcodeless rows never collide (null is not a key)', () => {
    expect(withoutLocalDupes([product({ id: 12 })], [item({ id: 1 })])).toHaveLength(1);
  });
});

describe('ProductPicker', () => {
  function setup(opts: { items?: Item[]; catalog?: Product[]; shopAvailable?: boolean } = {}) {
    const ref = { close: vi.fn() };
    const shops = {
      available: opts.shopAvailable ?? false,
      connect: vi.fn().mockResolvedValue(undefined),
      search: vi.fn().mockResolvedValue([]),
      fetchProduct: vi.fn().mockResolvedValue({
        source: 'waitrose',
        external_id: '062593',
        name: 'Waitrose Cheddar',
        brand: 'Waitrose',
        barcodes: [],
        image_url: 'https://cdn/img.jpg',
        display_price: null,
        categories: [],
      }),
    };
    const api = {
      searchProducts: vi.fn(() => of(opts.catalog ?? [])),
      importProduct: vi.fn(() => of(product({ id: 99, name: 'Waitrose Cheddar' }))),
      productImageUrl: (b: string) => `/api/products/${b}/image`,
      productImageByIdUrl: (id: number) => `/api/products/id/${id}/image`,
    };
    TestBed.configureTestingModule({
      imports: [ProductPicker],
      providers: [
        { provide: MatDialogRef, useValue: ref },
        { provide: MAT_DIALOG_DATA, useValue: { initialQuery: 'yog' } },
        { provide: LifeApi, useValue: api },
        { provide: Shops, useValue: shops },
        {
          provide: ItemsStore,
          useValue: { value: signal<Item[]>(opts.items ?? []), refresh: vi.fn() },
        },
        { provide: Feedback, useValue: { notify: vi.fn(), error: vi.fn() } },
      ],
    });
    return { fixture: TestBed.createComponent(ProductPicker), ref, shops, api };
  }

  it('picking an inventory hit closes with its full identity', () => {
    const it1 = item({
      id: 4,
      name: 'Greek Yoghurt',
      barcode: '5000000000001',
      product_id: 9,
      unit: 'pot',
    });
    const { fixture, ref } = setup({ items: [it1] });
    fixture.detectChanges();
    fixture.componentInstance.pickItem(it1);
    expect(ref.close).toHaveBeenCalledWith({
      name: 'Greek Yoghurt',
      barcode: '5000000000001',
      product_id: 9,
      unit: 'pot',
      category: 'food',
    });
  });

  it('outside the app, no shop tier is offered', () => {
    const { fixture } = setup();
    fixture.detectChanges();
    expect(fixture.componentInstance.shopProviders).toEqual([]);
    expect((fixture.nativeElement as HTMLElement).querySelector('.shop-tier')).toBeNull();
  });

  it('picking a shop hit fetches, imports, and closes linked to the import', async () => {
    const { fixture, ref, shops, api } = setup({ shopAvailable: true });
    fixture.detectChanges();
    const cmp = fixture.componentInstance;
    cmp.pickShop(cmp.shopProviders[0], {
      external_id: '062593',
      name: 'Cheddar',
      image_url: 'x',
    });
    await new Promise((r) => setTimeout(r));

    expect(shops.fetchProduct).toHaveBeenCalledWith(cmp.shopProviders[0], '062593');
    expect(api.importProduct).toHaveBeenCalledWith(
      expect.objectContaining({ source: 'waitrose', external_id: '062593' }),
    );
    expect(ref.close).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Waitrose Cheddar', product_id: 99 }),
    );
  });
});
