import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import { AsdaHit, ProductDetail } from '../../models';
import { ProductPage, eanMatch } from './product';

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

const hit = (over: Partial<AsdaHit>): AsdaHit => ({
  external_id: '1',
  name: 'A thing',
  brand: null,
  barcode: null,
  quantity_label: null,
  price_label: null,
  price: null,
  image_url: null,
  dietary: [],
  ...over,
});

describe('eanMatch', () => {
  // Asda's real answer for "Asda ES Balsamic Modena" — the product's own Open
  // Food Facts name. Asda ranks a RASPBERRY glaze first and the actual product
  // fourth, which is exactly why relevance order can't be trusted.
  const asdaHits = [
    hit({ external_id: '2266257', barcode: '5050854946264', name: 'Glaze with Balsamic Vinegar of Modena 250ml' }),
    hit({ external_id: '9020293', barcode: '5063089281598', name: 'Raspberry Glaze with Balsamic Vinegar of Modena' }),
    hit({ external_id: '1554788', barcode: '27595466', name: 'Balsamic Vinegar of Modena 250ml' }),
    hit({ external_id: '9020290', barcode: '5063089281581', name: 'Extra Special Balsamic Vinegar of Modena 250ml', price_label: '£8.00' }),
  ];

  it('takes the barcode match, not the shop’s top hit', () => {
    const match = eanMatch(asdaHits, '5063089281581');
    expect(match?.external_id).toBe('9020290');
    expect(match?.name).toBe('Extra Special Balsamic Vinegar of Modena 250ml');
  });

  it('refuses look-alikes: no barcode match means no match at all', () => {
    // A Spanish gourmet olive oil against Asda's own-brand/Filippo Berio oils —
    // same words, different products. Linking them would invent a fact.
    const oils = [
      hit({ barcode: '5057172338665', name: 'Extra Virgin Olive Oil 1 Litre' }),
      hit({ barcode: '8002210500204', name: 'Filippo Berio Extra Virgin Olive Oil 500ml' }),
    ];
    expect(eanMatch(oils, '8410660200013')).toBeNull();
  });

  it('never matches a barcodeless hit', () => {
    expect(eanMatch([hit({ barcode: null })], '5063089281581')).toBeNull();
  });
});

describe('ProductPage', () => {
  afterEach(() => TestBed.resetTestingModule());

  function setup(detail: ProductDetail = DETAIL, asdaHits: AsdaHit[] = []) {
    const api = {
      getProductDetail: vi.fn(() => of(detail)),
      productImageByIdUrl: (id: number) => `/api/products/id/${id}/image`,
      searchAsda: vi.fn(() => of(asdaHits)),
      syncListing: vi.fn(() => of(detail.product)),
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

  /** A product Open Food Facts knows but no shop lists yet — the state the
   *  "Find at Asda" action exists for. */
  const UNLISTED: ProductDetail = {
    ...DETAIL,
    product: { ...DETAIL.product, barcode: '5063089281581', name: 'Asda ES Balsamic Modena' },
    listings: [DETAIL.listings[0]], // the off listing only
    prices: [],
  };

  // The lookup is offered only when it can answer truthfully:
  it('hides the Asda lookup when Asda already lists the product', () => {
    expect(setup().page.canFindAtAsda()).toBe(false);
  });

  it('hides the Asda lookup with no barcode — there’d be nothing to match on', () => {
    const detail = { ...UNLISTED, product: { ...UNLISTED.product, barcode: null } };
    expect(setup(detail).page.canFindAtAsda()).toBe(false);
  });

  it('offers the Asda lookup for a barcoded product no shop lists yet', () => {
    expect(setup(UNLISTED).page.canFindAtAsda()).toBe(true);
  });

  it('finds the product Asda ranks fourth, and adds it under its own barcode', () => {
    const asdaHits = [
      hit({ external_id: '9020293', barcode: '5063089281598', name: 'Raspberry Glaze' }),
      hit({
        external_id: '9020290',
        barcode: '5063089281581',
        name: 'Extra Special Balsamic Vinegar of Modena 250ml',
        price: { amount_minor: 800, currency: 'GBP', unit_amount_minor: null, unit_measure: null, region: 'EN' },
      }),
    ];
    const { page, api } = setup(UNLISTED, asdaHits);
    page.findAtAsda();
    // Searched by the only name we have — Open Food Facts' cryptic one.
    expect(api.searchAsda).toHaveBeenCalledWith('Asda ES Balsamic Modena');
    expect(page.shopLookup()).toBe('found');
    expect(page.shopMatch()?.external_id).toBe('9020290');

    // Attaching hands the backend only the listing's identity — it re-fetches
    // shop-side and re-checks the barcode itself, so no facts are client-supplied.
    page.attach(page.shopMatch()!);
    expect(api.syncListing).toHaveBeenCalledWith(42, 'asda', '9020290');
    expect(page.shopLookup()).toBe('idle'); // panel resets; the page reloads
  });

  it('refreshes a shop row only when asked, by the listing it names', () => {
    const { page, api } = setup();
    const asda = page.buyRows().find((r) => r.label === 'Asda')!;
    expect(api.syncListing).not.toHaveBeenCalled(); // nothing on load — no timer
    page.refresh(asda);
    expect(api.syncListing).toHaveBeenCalledWith(42, 'asda', '9346702');
  });

  it('reports an honest "not stocked" rather than offering a look-alike', () => {
    const { page } = setup(UNLISTED, [
      hit({ external_id: '1', barcode: '5057172338665', name: 'Extra Virgin Olive Oil 1 Litre' }),
    ]);
    page.findAtAsda();
    expect(page.shopLookup()).toBe('none');
    expect(page.shopMatch()).toBeNull();
  });

  it('distinguishes a failed search from an empty one', () => {
    const api = {
      getProductDetail: vi.fn(() => of(UNLISTED)),
      productImageByIdUrl: () => '',
      searchAsda: vi.fn(() => throwError(() => new HttpErrorResponse({ status: 0 }))),
    };
    TestBed.configureTestingModule({
      imports: [ProductPage],
      providers: [{ provide: LifeApi, useValue: api }],
    });
    const fixture = TestBed.createComponent(ProductPage);
    fixture.componentRef.setInput('id', '42');
    fixture.detectChanges();
    fixture.componentInstance.findAtAsda();
    expect(fixture.componentInstance.shopLookup()).toBe('error');
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
