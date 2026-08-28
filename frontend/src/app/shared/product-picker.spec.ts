import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { signal } from '@angular/core';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from './feedback';
import { LifeApi } from '../life-api';
import { AsdaHit, Item, PriceInput, Product } from '../models';
import { Shops } from '../shop';
import { ItemsStore } from '../stores/catalog';
import { ProductPicker, localHits, packPrefill, withoutLocalDupes } from './product-picker';

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
  pack: null,
  source: 'off',
  external_id: null,
  name_source: 'off',
  image_source: null,
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

describe('packPrefill', () => {
  it('fills a measured pack as a quantity and its unit', () => {
    expect(packPrefill({ value: 950, unit: 'g' })).toEqual({ quantity: 950, unit: 'g' });
    expect(packPrefill({ value: 330, unit: 'ml' })).toEqual({ quantity: 330, unit: 'ml' });
  });

  it('fills a countable pack as a bare number', () => {
    // "1", not "1 count": the item model spells a countable thing with the unit
    // left empty, and anything in that box would stop it matching a line that
    // says the same thing by leaving it empty too.
    expect(packPrefill({ value: 1, unit: 'count' })).toEqual({ quantity: 1, unit: null });
  });

  it('fills nothing for a pack that could not be read', () => {
    // The raw label goes on being displayed; what it must not do is put a
    // number nobody measured into the cupboard.
    expect(packPrefill(null)).toEqual({ quantity: null, unit: null });
  });
});

describe('ProductPicker', () => {
  function setup(
    opts: {
      items?: Item[];
      catalog?: Product[];
      asda?: AsdaHit[];
      shopAvailable?: boolean;
      imported?: Product;
    } = {},
  ) {
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
        quantity_label: '200g',
        image_url: 'https://cdn/img.jpg',
        display_price: { amount: 2.65, currencyCode: 'GBP' },
        display_price_label: '£2.65',
        categories: [],
      }),
    };
    const api = {
      searchProducts: vi.fn(() => of(opts.catalog ?? [])),
      searchAsda: vi.fn(() => of(opts.asda ?? [])),
      importProduct: vi.fn(() =>
        of(opts.imported ?? product({ id: 99, name: 'Waitrose Cheddar' })),
      ),
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
      // The unit an existing row measures itself in, but never its quantity:
      // how much is left of the pot you own is not how much a new one holds.
      quantity: null,
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

  it('picking an Asda hit imports it and closes with the hit’s barcode', async () => {
    const hit: AsdaHit = {
      external_id: '7690049',
      name: 'Lurpak Spreadable 400g',
      brand: 'Lurpak',
      barcode: '5740900404465',
      quantity_label: '400G',
      price_label: '£3.57',
      price: {
        amount_minor: 357,
        currency: 'GBP',
        unit_amount_minor: 892,
        unit_measure: 'KG',
        region: 'EN',
      },
      image_url: 'https://asdagroceries.scene7.com/is/image/asdagroceries/5740900404465?$ProdList$',
      dietary: [],
    };
    const { fixture, ref, api } = setup({ asda: [hit] });
    fixture.detectChanges();
    fixture.componentInstance.pickAsda(hit);
    await new Promise((r) => setTimeout(r));

    // The price rides along to be recorded as an observation, and so does the
    // pack — dropping it here left the imported row unmeasured, so a pick from
    // the shop tier could not fill a quantity the hit had been carrying all along.
    expect(api.importProduct).toHaveBeenCalledWith(
      expect.objectContaining({
        source: 'asda',
        external_id: '7690049',
        quantity_label: '400G',
        price: hit.price,
      }),
    );
    // The imported catalogue row is barcodeless; the hit's EAN must still ride
    // back so the shopping row is barcoded.
    expect(ref.close).toHaveBeenCalledWith(
      expect.objectContaining({ product_id: 99, barcode: '5740900404465' }),
    );
  });

  it('an imported shop product hands its pack size straight to the form', async () => {
    // The round trip the shop tier depends on: the hit's label goes up, the
    // server stores it and reads it back as an amount, and the pick carries the
    // amount. Asserted through the import so a server that stopped promoting
    // the label would show up here rather than as a quietly empty form field.
    const hit: AsdaHit = {
      external_id: '7690049',
      name: 'Lurpak Spreadable 400g',
      brand: 'Lurpak',
      barcode: '5740900404465',
      quantity_label: '400G',
      price_label: null,
      price: null,
      image_url: null,
      dietary: [],
    };
    const { fixture, ref } = setup({
      asda: [hit],
      imported: product({ id: 99, quantity_label: '400G', pack: { value: 400, unit: 'g' } }),
    });
    fixture.detectChanges();
    fixture.componentInstance.pickAsda(hit);
    await new Promise((r) => setTimeout(r));

    expect(ref.close).toHaveBeenCalledWith(
      expect.objectContaining({ quantity: 400, unit: 'g' }),
    );
  });

  it('the Asda tier is backend-backed, so it shows even outside the app', async () => {
    const hit: AsdaHit = {
      external_id: '1',
      name: 'Asda thing',
      brand: null,
      barcode: null,
      quantity_label: null,
      price_label: null,
      price: null,
      image_url: null,
      dietary: [],
    };
    const { fixture } = setup({ asda: [hit] });
    fixture.detectChanges();
    // The tier is driven by the same 250ms-debounced query as the catalog tier;
    // let it settle before asserting.
    await new Promise((r) => setTimeout(r, 300));
    fixture.detectChanges();
    expect(fixture.componentInstance.asda()).toEqual([hit]);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Asda thing');
    expect(fixture.componentInstance.shopProviders).toEqual([]); // no app bridge, yet Asda shows
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
      expect.objectContaining({
        source: 'waitrose',
        external_id: '062593',
        // The pack the shop sells. Dropping it here is invisible — the import
        // succeeds and the product simply arrives unmeasured, so stock linked
        // from a Waitrose pick starts out not knowing how much it holds.
        quantity_label: '200g',
        // The shop's own quote, recorded here as it is everywhere else — a price
        // this path dropped would make "cheapest shop" depend on which screen
        // you linked the product from. Spelled out rather than matched loosely:
        // the whole shape is what the import path promises.
        price: {
          amount_minor: 265,
          currency: 'GBP',
          unit_amount_minor: null,
          unit_measure: null,
          region: null,
        } satisfies PriceInput,
      }),
    );
    expect(ref.close).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Waitrose Cheddar', product_id: 99 }),
    );
  });

  it('a picked catalog product carries its pack size to the form', async () => {
    // The point of parsing the label: a product knew its pack size all along
    // and could not hand it over, so stock linked to it started out untracked.
    const { fixture, ref } = setup({
      catalog: [product({ id: 7, name: 'Greek Yoghurt', quantity_label: '950g', pack: { value: 950, unit: 'g' } })],
    });
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r));

    fixture.componentInstance.pickProduct(
      product({ id: 7, name: 'Greek Yoghurt', quantity_label: '950g', pack: { value: 950, unit: 'g' } }),
    );

    expect(ref.close).toHaveBeenCalledWith(
      expect.objectContaining({ product_id: 7, quantity: 950, unit: 'g' }),
    );
  });
});
