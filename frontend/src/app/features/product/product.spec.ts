import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import { ProductDetail } from '../../models';
import { ProductPage } from './product';

const DETAIL: ProductDetail = {
  product: {
    id: 42,
    barcode: '5000328042732',
    name: 'Quaker Oat So Simple Original',
    brand: 'Quaker',
    quantity_label: '22x27G',
    source: 'off',
    external_id: '5000328042732',
    name_source: 'asda',
    has_image: true,
  },
  listings: [
    { source: 'off', external_id: '5000328042732', url: 'https://world.openfoodfacts.org/product/5000328042732', raw_name: 'oat so simple' },
    { source: 'asda', external_id: '9346702', url: 'https://www.asda.com/groceries/product/9346702', raw_name: 'Quaker Oat So Simple Original' },
    { source: 'waitrose', external_id: '271105', url: 'https://www.waitrose.com/ecom/products/x/271105', raw_name: 'Oat So Simple' },
  ],
  prices: [
    // One row per shop, cheapest first, each naming its listing — as the backend
    // guarantees. Waitrose quotes no unit price.
    { source: 'waitrose', external_id: '271105', amount_minor: 450, currency: 'GBP',
      unit_amount_minor: null, unit_measure: null, region: null, observed_at: Date.now() },
    { source: 'asda', external_id: '9346702', amount_minor: 475, currency: 'GBP',
      unit_amount_minor: 800, unit_measure: 'KG', region: 'EN', observed_at: Date.now() - 2 * 86_400_000 },
  ],
  facts: {
    nutrition: {
      basis: '100g',
      serving_size: '40 g',
      energy_kj: 1500,
      energy_kcal: 356,
      fat_g: 6.5,
      saturates_g: 1.2,
      carbohydrate_g: 60,
      sugars_g: 1,
      fibre_g: null,
      protein_g: 11,
      salt_g: 0.1,
      extra: { sodium: 0.04 },
    },
    ingredients: 'Wholegrain oats (95%), sugar',
    allergens: [
      { allergen: 'gluten', presence: 'contains' },
      { allergen: 'nuts', presence: 'may_contain' },
    ],
    dietary: [
      { flag: 'vegan', value: 'no' },
      { flag: 'vegetarian', value: 'yes' },
      { flag: 'palm_oil_free', value: 'maybe' },
    ],
  },
};

describe('ProductPage', () => {
  afterEach(() => TestBed.resetTestingModule());

  function setup(detail: ProductDetail = DETAIL) {
    const api = {
      getProductDetail: vi.fn(() => of(detail)),
      productImageByIdUrl: (id: number) => `/api/products/id/${id}/image`,
    };
    TestBed.configureTestingModule({
      imports: [ProductPage],
      providers: [{ provide: LifeApi, useValue: api }],
    });
    const fixture = TestBed.createComponent(ProductPage);
    fixture.componentRef.setInput('id', '42');
    fixture.detectChanges();
    return { fixture, api, page: fixture.componentInstance };
  }

  it('loads the detail for the routed id and shows the hero image', () => {
    const { api, page } = setup();
    expect(api.getProductDetail).toHaveBeenCalledWith(42);
    expect(page.imageUrl()).toBe('/api/products/id/42/image');
    expect(page.subtitle()).toBe('Quaker · 22x27G');
  });

  it('lists shops cheapest first with deep links, skipping the off listing', () => {
    const rows = setup().page.buyRows();
    expect(rows.map((r) => r.label)).toEqual(['Waitrose', 'Asda']);
    expect(rows[0].price).toBe('£4.50');
    expect(rows[0].perUnit).toBeNull();
    expect(rows[0].url).toBe('https://www.waitrose.com/ecom/products/x/271105');
    expect(rows[1].price).toBe('£4.75');
    expect(rows[1].perUnit).toBe('£8.00/KG');
    expect(rows[1].observed).toBe('2 days ago');
  });

  it('links a priceless shop listing and keeps off as attribution only', () => {
    const { page } = setup({
      ...DETAIL,
      prices: [],
    });
    const rows = page.buyRows();
    expect(rows.map((r) => r.label)).toEqual(['Asda', 'Waitrose']);
    expect(rows[0].price).toBeNull();
    expect(page.offUrl()).toBe('https://world.openfoodfacts.org/product/5000328042732');
  });

  it('links the exact listing a price came from, and rows stay uniquely keyed', () => {
    // A shop can list one product twice (two Asda CINs on one EAN). The backend
    // collapses the price to that shop's cheapest listing; the link must follow
    // the price to THAT listing, and the row keys must stay distinct — keying by
    // the display label would collide and break the @for track.
    const { page } = setup({
      ...DETAIL,
      listings: [
        ...DETAIL.listings,
        { source: 'asda', external_id: '5511122', url: 'https://www.asda.com/groceries/product/5511122', raw_name: 'Quaker Oat So Simple (relist)' },
      ],
      prices: [
        { source: 'asda', external_id: '5511122', amount_minor: 399, currency: 'GBP',
          unit_amount_minor: null, unit_measure: null, region: 'EN', observed_at: Date.now() },
      ],
    });
    const rows = page.buyRows();
    expect(rows.filter((r) => r.label === 'Asda').length).toBe(1);
    expect(rows[0].url).toBe('https://www.asda.com/groceries/product/5511122');
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
    // Waitrose has no price here, so it appears once as a plain link.
    expect(rows.find((r) => r.label === 'Waitrose')?.price).toBeNull();
  });

  it('builds the UK panel in order, omitting undeclared rows', () => {
    const rows = setup().page.nutrientRows();
    expect(rows.map((r) => r.label)).toEqual([
      'Energy',
      'Fat',
      'of which saturates',
      'Carbohydrate',
      'of which sugars',
      'Protein',
      'Salt',
    ]);
    expect(rows[0].value).toBe('1500 kJ / 356 kcal');
    expect(rows[1].value).toBe('6.5 g');
    expect(rows[2].sub).toBe(true);
  });

  it('splits allergens by presence and humanizes dietary flags', () => {
    const { page } = setup();
    expect(page.contains()).toEqual(['Gluten']);
    expect(page.mayContain()).toEqual(['Nuts']);
    expect(page.dietary()).toEqual([
      { label: 'Vegan', value: 'no' },
      { label: 'Vegetarian', value: 'yes' },
      { label: 'Palm oil free', value: 'maybe' },
    ]);
  });

  /** Mount the page with an API that fails the detail load with `err`. */
  function setupFailing(err: unknown, id = '42') {
    const api = {
      getProductDetail: vi.fn(() => throwError(() => err)),
      productImageByIdUrl: () => '',
    };
    TestBed.configureTestingModule({
      imports: [ProductPage],
      providers: [{ provide: LifeApi, useValue: api }],
    });
    const fixture = TestBed.createComponent(ProductPage);
    fixture.componentRef.setInput('id', id);
    fixture.detectChanges();
    return { page: fixture.componentInstance, api };
  }

  it('says the product is missing on a 404 — never blames the connection', () => {
    const { page } = setupFailing(new HttpErrorResponse({ status: 404 }));
    expect(page.error()).toBe(true);
    expect(page.loading()).toBe(false);
    expect(page.errorText()).toBe('That product isn’t in the catalogue.');
  });

  it('blames the connection only when actually offline', () => {
    // withFetch() reports a dropped connection as status 0.
    const { page } = setupFailing(new HttpErrorResponse({ status: 0 }));
    expect(page.errorText()).toContain('offline');
  });

  it('distinguishes a server fault from a missing product', () => {
    const { page } = setupFailing(new HttpErrorResponse({ status: 500 }));
    expect(page.errorText()).toBe('The server couldn’t load this product.');
  });

  it('rejects a junk id without asking the server about it', () => {
    const { page, api } = setupFailing(new HttpErrorResponse({ status: 404 }), 'not-a-number');
    expect(api.getProductDetail).not.toHaveBeenCalled();
    expect(page.error()).toBe(true);
    expect(page.errorText()).toBe('That product link isn’t valid.');
  });
});
