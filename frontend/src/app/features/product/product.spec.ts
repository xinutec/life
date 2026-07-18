import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import { AsdaHit, ProductDetail, ShopFind } from '../../models';
import { Shops } from '../../shop';
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
  reconciliation: { fields: [] },
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

describe('ProductPage', () => {
  afterEach(() => TestBed.resetTestingModule());

  function setup(
    detail: ProductDetail = DETAIL,
    find: ShopFind = { hit: null, from_cache: false },
    shops: Partial<Shops> = { available: false },
  ) {
    const api = {
      getProductDetail: vi.fn(() => of(detail)),
      productImageByIdUrl: (id: number) => `/api/products/id/${id}/image`,
      findAtShop: vi.fn(() => of(find)),
      syncListing: vi.fn(() => of(detail.product)),
      // Answers with the same detail but no divergences left (the settled state).
      reconcile: vi.fn(() => of({ ...detail, reconciliation: { fields: [] } })),
      submitFacts: vi.fn(() => of({ ...detail, reconciliation: { fields: [] } })),
    };
    TestBed.configureTestingModule({
      imports: [ProductPage],
      providers: [
        { provide: LifeApi, useValue: api },
        { provide: Shops, useValue: shops },
      ],
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

  it('shows the confirmed match and adds it under its own barcode', () => {
    // The backend hands back the one hit it confirmed by EAN — the client no
    // longer sifts a result list, so there's nothing to rank here.
    const confirmed = hit({
      external_id: '9020290',
      barcode: '5063089281581',
      name: 'Extra Special Balsamic Vinegar of Modena 250ml',
      price: { amount_minor: 800, currency: 'GBP', unit_amount_minor: null, unit_measure: null, region: 'EN' },
    });
    const { page, api } = setup(UNLISTED, { hit: confirmed, from_cache: false });
    page.findAtAsda();
    // Asks the backend about THIS product at THIS shop; it owns both the cache
    // check and the barcode match (products::asda::match_barcode).
    expect(api.findAtShop).toHaveBeenCalledWith(42, 'asda');
    expect(page.shopLookup()).toBe('found');
    expect(page.shopMatch()?.external_id).toBe('9020290');
    expect(page.fromCache()).toBe(false);

    // Attaching hands the backend only the listing's identity — it re-fetches
    // shop-side and re-checks the barcode itself, so no facts are client-supplied.
    page.attach(page.shopMatch()!);
    expect(api.syncListing).toHaveBeenCalledWith(42, 'asda', '9020290');
    expect(page.shopLookup()).toBe('idle'); // panel resets; the page reloads
  });

  it('previews the match — picture and pack details — before you commit to Add', () => {
    // "See it first": the found panel renders Asda's own picture and a
    // brand·size subtitle from the hit itself, so the shopper can size up the
    // product without importing it. Straight from the shop URL — no DB write.
    const confirmed = hit({
      external_id: '9020290',
      name: 'Extra Special Balsamic Vinegar of Modena 250ml',
      brand: 'Asda Extra Special',
      quantity_label: '250ml',
      image_url: 'https://s7g10.scene7.com/is/image/asda/9020290?$ProdList$',
    });
    const { fixture, page } = setup(UNLISTED, { hit: confirmed, from_cache: false });
    page.findAtAsda();
    expect(page.hitSubtitle(confirmed)).toBe('Asda Extra Special · 250ml');
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const img = el.querySelector<HTMLImageElement>('.match-img');
    expect(img?.getAttribute('src')).toBe(confirmed.image_url);
    expect(el.querySelector('.match')?.textContent).toContain('250ml');
  });

  it('falls back to the verified tick when a match has no picture', () => {
    const confirmed = hit({ external_id: '7', name: 'Pictureless thing', image_url: null });
    const { fixture, page } = setup(UNLISTED, { hit: confirmed, from_cache: false });
    page.findAtAsda();
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.match-img')).toBeNull();
    expect(el.querySelector('.match .ok')).not.toBeNull();
    expect(page.hitSubtitle(confirmed)).toBe(''); // nothing to size up, so nothing shown
  });

  it('refreshes a shop row only when asked, by the listing it names', () => {
    const { page, api } = setup();
    const asda = page.buyRows().find((r) => r.label === 'Asda')!;
    expect(api.syncListing).not.toHaveBeenCalled(); // nothing on load — no timer
    page.refresh(asda);
    expect(api.syncListing).toHaveBeenCalledWith(42, 'asda', '9346702');
  });

  it('reports an honest "not stocked" rather than offering a look-alike', () => {
    // No confirmed hit: the backend checked every result and none carried this
    // EAN. That's a real answer, and it must read as one — not as an error, and
    // never as a nearest-name suggestion.
    const { page } = setup(UNLISTED, { hit: null, from_cache: false });
    page.findAtAsda();
    expect(page.shopLookup()).toBe('none');
    expect(page.shopMatch()).toBeNull();
  });

  it('says so when the answer came from memory rather than the shop', () => {
    // A cache hit is still a real match, but the page shouldn't pretend it just
    // asked Asda — and a remembered hit carries no price, by design.
    const remembered = hit({
      external_id: '9020290',
      barcode: '5063089281581',
      name: 'Extra Special Balsamic Vinegar of Modena 250ml',
      price_label: null,
    });
    const { page } = setup(UNLISTED, { hit: remembered, from_cache: true });
    page.findAtAsda();
    expect(page.shopLookup()).toBe('found');
    expect(page.fromCache()).toBe(true);
  });

  it('distinguishes a failed search from an empty one', () => {
    const api = {
      getProductDetail: vi.fn(() => of(UNLISTED)),
      productImageByIdUrl: () => '',
      findAtShop: vi.fn(() => throwError(() => new HttpErrorResponse({ status: 0 }))),
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

  // --- Reconciliation (approve where sources disagree) ---

  const DIVERGENT: ProductDetail = {
    ...DETAIL,
    reconciliation: {
      fields: [
        {
          field: 'brand',
          label: 'Brand',
          current: 'OFF Brand',
          candidates: [{ source: 'asda', value: 'Asda Brand' }],
        },
        {
          field: 'quantity_label',
          label: 'Pack size',
          current: '250ML',
          candidates: [{ source: 'asda', value: '250ml' }],
        },
      ],
    },
  };

  it('shows nothing to review when the sources agree', () => {
    const { fixture } = setup(); // DETAIL has an empty reconciliation
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.reconcile')).toBeNull();
  });

  it('surfaces each disagreeing field with the current value and each source’s', () => {
    const { fixture, page } = setup(DIVERGENT);
    const el = fixture.nativeElement as HTMLElement;
    const panel = el.querySelector('.reconcile');
    expect(panel).not.toBeNull();
    expect(panel?.textContent).toContain('Brand');
    // Both the pack-size disagreement (the real 250ML vs 250ml case) and its
    // candidate are on offer.
    expect(panel?.textContent).toContain('250ML');
    expect(panel?.textContent).toContain('250ml');
    expect(page.reconFields().length).toBe(2);
  });

  it('applies a per-field decision: adopt one, keep the other', () => {
    const { api, page } = setup(DIVERGENT);
    page.setChoice('brand', 'asda'); // adopt Asda's brand
    // quantity_label left untouched → defaults to keep
    page.applyReconcile();
    expect(api.reconcile).toHaveBeenCalledWith(42, [
      { field: 'brand', choice: 'asda' },
      { field: 'quantity_label', choice: 'keep' },
    ]);
  });

  it('defaults every field to keep, so applying without a pick changes nothing', () => {
    const { api, page } = setup(DIVERGENT);
    page.applyReconcile();
    expect(api.reconcile).toHaveBeenCalledWith(42, [
      { field: 'brand', choice: 'keep' },
      { field: 'quantity_label', choice: 'keep' },
    ]);
  });

  it('lets you type our own name — a user-owned correction when every shop is wrong', () => {
    const { api, page } = setup();
    page.startEditName();
    expect(page.nameDraft()).toBe('Quaker Oat So Simple Original'); // prefilled from current
    page.nameDraft.set('  Oatly Barista  ');
    page.saveName();
    expect(api.reconcile).toHaveBeenCalledWith(42, [
      { field: 'name', choice: 'user', value: 'Oatly Barista' }, // trimmed
    ]);
  });

  it('won’t save an empty name', () => {
    const { api, page } = setup();
    page.startEditName();
    page.nameDraft.set('   ');
    page.saveName();
    expect(api.reconcile).not.toHaveBeenCalled();
  });

  it('lets you type our own brand + pack — the shop-casing fix ("250ML" → "250ml")', () => {
    const { api, page } = setup();
    page.startEditDetails();
    expect(page.brandDraft()).toBe('Quaker'); // prefilled from current
    expect(page.packDraft()).toBe('22x27G');
    page.brandDraft.set('  Oatly  ');
    page.packDraft.set('  250ml  ');
    page.saveDetails();
    expect(api.reconcile).toHaveBeenCalledWith(42, [
      { field: 'brand', choice: 'user', value: 'Oatly' }, // trimmed
      { field: 'quantity_label', choice: 'user', value: '250ml' },
    ]);
  });

  it('only sends the field you actually changed', () => {
    const { api, page } = setup();
    page.startEditDetails();
    page.packDraft.set('250ml'); // brand left as-is
    page.saveDetails();
    expect(api.reconcile).toHaveBeenCalledWith(42, [
      { field: 'quantity_label', choice: 'user', value: '250ml' },
    ]);
  });

  it('changes nothing when neither detail was touched', () => {
    const { api, page } = setup();
    page.startEditDetails();
    page.saveDetails();
    expect(api.reconcile).not.toHaveBeenCalled();
    expect(page.editingDetails()).toBe(false); // just closes
  });

  it('hides the Asda full-details action outside the app (no shop bridge)', () => {
    const { fixture, page } = setup(); // shops.available === false by default
    expect(page.canGetAsdaFacts()).toBe(false); // even though DETAIL has an Asda listing
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.asda-facts')).toBeNull();
  });

  it('offers Asda full details in the app and stores the fetched blob', async () => {
    const fetchFacts = vi.fn(() =>
      Promise.resolve({ ean: '5000328042732', blob: '{"calculatedNutrition":[]}' }),
    );
    const { api, page } = setup(DETAIL, undefined, { available: true, fetchFacts });

    expect(page.canGetAsdaFacts()).toBe(true);
    page.getAsdaFacts();
    await new Promise((r) => setTimeout(r)); // flush the fetch → submit chain

    // Fetched by the product's own Asda listing (CIN 9346702), parsed server-side.
    expect(fetchFacts).toHaveBeenCalledWith(expect.objectContaining({ id: 'asda' }), '9346702');
    expect(api.submitFacts).toHaveBeenCalledWith(42, {
      source: 'asda',
      ean: '5000328042732',
      blob: '{"calculatedNutrition":[]}',
    });
    expect(page.fetchingFacts()).toBe(false);
  });
});
