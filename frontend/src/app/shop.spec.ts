import { afterEach, describe, expect, it } from 'vitest';

import { PriceInput } from './models';
import { ShopProduct, ShopProvider, Shops, penceFromLabel, shopPrice } from './shop';
import { WAITROSE } from './shops/waitrose';

// The native bridge lives on window; fake it per-test.
interface TestWin {
  ShopBridge?: unknown;
  __shopResolve?: (id: string, res: unknown) => void;
  __shopConnected?: (id: string | null) => void;
}
const w = window as unknown as TestWin;

// Records the last (url, js, requestId) the bridge was asked to run.
let lastRun: { url: string; js: string; id: string } | undefined;
let lastConnect: { loginUrl: string; id: string } | undefined;

/** The native side is a message port now — origin-scoped, so it can't be reached
 *  from an embedded frame the way the old injected object could. Requests arrive
 *  as JSON; results still come back through `window.__shopResolve`, because a
 *  hidden WebView's answer lands long after the message that asked for it. */
function fakeBridge() {
  lastRun = undefined;
  lastConnect = undefined;
  w.ShopBridge = {
    postMessage: (raw: string) => {
      const req: unknown = JSON.parse(raw);
      if (!isRecord(req)) return;
      const id = String(req['requestId']);
      if (req['op'] === 'run') {
        lastRun = { url: String(req['url']), js: String(req['extractorJs']), id };
      } else if (req['op'] === 'connect') {
        lastConnect = { loginUrl: String(req['loginUrl']), id };
      }
    },
  };
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null;
}

// A stand-in for the bridge mechanics, not for Waitrose itself — but its `id`
// still has to name a real source, because that is what a provider is allowed to
// be. The URLs below are deliberately fake; only the plumbing is under test.
const provider: ShopProvider = {
  id: 'waitrose',
  displayName: 'Test',
  loginUrl: 'https://x.test/',
  search: (q) => ({ url: `https://x.test/s?q=${q}`, js: 'SEARCH_JS' }),
  product: (id) => ({ url: `https://x.test/p/${id}`, js: 'PRODUCT_JS' }),
};

describe('Shops bridge service', () => {
  afterEach(() => {
    delete w.ShopBridge;
    delete w.__shopResolve;
    delete w.__shopConnected;
  });

  it('available is false in a plain browser', () => {
    expect(new Shops().available).toBe(false);
  });

  it('search runs the provider url+js and resolves its candidates', async () => {
    fakeBridge();
    const svc = new Shops();
    const p = svc.search(provider, 'milk');
    expect(lastRun?.url).toBe('https://x.test/s?q=milk');
    expect(lastRun?.js).toBe('SEARCH_JS');
    w.__shopResolve!(lastRun!.id, {
      ok: true,
      candidates: [{ external_id: '1', name: 'Milk', image_url: 'x' }],
    });
    await expect(p).resolves.toEqual([{ external_id: '1', name: 'Milk', image_url: 'x' }]);
  });

  it('fetchProduct resolves the product', async () => {
    fakeBridge();
    const svc = new Shops();
    const p = svc.fetchProduct(provider, '062593');
    expect(lastRun?.url).toBe('https://x.test/p/062593');
    w.__shopResolve!(lastRun!.id, {
      ok: true,
      product: { source: 'test', external_id: '062593', name: 'Milk' },
    });
    await expect(p).resolves.toMatchObject({ external_id: '062593', name: 'Milk' });
  });

  it('rejects when the bridge returns an error', async () => {
    fakeBridge();
    const svc = new Shops();
    const p = svc.search(provider, 'x');
    w.__shopResolve!(lastRun!.id, { ok: false, error: 'boom' });
    await expect(p).rejects.toThrow('boom');
  });

  it('connect opens the login and resolves when the overlay closes', async () => {
    fakeBridge();
    const svc = new Shops();
    const p = svc.connect(provider);
    expect(lastConnect?.loginUrl).toBe('https://x.test/');
    w.__shopConnected!(lastConnect!.id);
    await expect(p).resolves.toBeUndefined();
  });

  it('methods reject when there is no bridge', async () => {
    await expect(new Shops().search(provider, 'x')).rejects.toThrow(/only available in the app/);
  });
});

describe('Waitrose provider', () => {
  it('builds a search url + extractor that targets waitrose.com', () => {
    const { url, js } = WAITROSE.search('cheddar');
    expect(url).toContain('waitrose.com/ecom/shop/search?searchTerm=cheddar');
    expect(js).toContain('AndroidShop.result');
  });

  it('product() guards the lineNumber and targets the SUMMARY API', () => {
    const { js } = WAITROSE.product('062593');
    expect(js).toContain('products-prod/v1/products/062593?view=SUMMARY');
    expect(js).toContain('window.__authToken');
    expect(() => WAITROSE.product('not-a-number')).toThrow(/invalid/);
  });

  it('the extractor reads the pack size off the weights block', () => {
    // Waitrose puts it on `weights.sizeDescription` ("42g", "100g") rather than
    // beside the name, so a reader looking near `name` finds nothing and the
    // product arrives unmeasured — which is what this used to do.
    const { js } = WAITROSE.product('062593');
    expect(js).toContain('w.sizeDescription');
    expect(js).toContain('quantity_label');
  });

  it('the extractor carries the formatted price, not just the number', () => {
    // Without it there is nothing to check the amount's unit against, and
    // shopPrice would (rightly) refuse to record anything at all.
    const { js } = WAITROSE.product('062593');
    expect(js).toContain('display_price_label');
    expect(js).toContain('pr.displayPrice');
  });
});

describe('penceFromLabel', () => {
  it('reads both shapes a UK shop renders', () => {
    expect(penceFromLabel('£2.50')).toBe(250);
    expect(penceFromLabel('£12')).toBe(1200);
    expect(penceFromLabel('£1.05')).toBe(105);
    expect(penceFromLabel('£0.85')).toBe(85);
    expect(penceFromLabel('85p')).toBe(85);
    expect(penceFromLabel(' £2.50 ')).toBe(250);
  });

  it('refuses to guess at anything else', () => {
    // A per-unit price, a range or a bare number is not this product's price,
    // and reading one as if it were would be worse than having none.
    for (const junk of ['', 'free', '2.50', '$2.50', '£2.50/kg', '£2.50 - £4.00', '250']) {
      expect(penceFromLabel(junk), junk).toBeNull();
    }
  });
});

describe('shopPrice', () => {
  function product(over: Partial<ShopProduct> = {}): ShopProduct {
    return {
      source: 'waitrose',
      external_id: '062593',
      name: 'Cheddar',
      brand: null,
      barcodes: [],
      quantity_label: null,
      image_url: null,
      display_price: { amount: 2.5, currencyCode: 'GBP' },
      display_price_label: '£2.50',
      categories: [],
      ...over,
    };
  }

  it("records the quote when the shop's own two fields agree", () => {
    expect(shopPrice(product())).toEqual<PriceInput>({
      amount_minor: 250,
      currency: 'GBP',
      unit_amount_minor: null,
      unit_measure: null,
      region: null,
    });
  });

  it('records nothing when the shop quoted nothing', () => {
    expect(shopPrice(product({ display_price: null }))).toBeNull();
    expect(shopPrice(product({ display_price: { amount: 0, currencyCode: 'GBP' } }))).toBeNull();
  });

  it('records nothing when the amount is in the other unit', () => {
    // The whole point: `amount` carries no unit, so 250 could mean £250 or
    // £2.50. The shop's own label says which, and here it disagrees by 100×.
    expect(shopPrice(product({ display_price: { amount: 250, currencyCode: 'GBP' } }))).toBeNull();
  });

  it('records nothing when there is no label to check against', () => {
    // An unconfirmable price is not a price. Missing is visible; wrong is not.
    expect(shopPrice(product({ display_price_label: null }))).toBeNull();
    expect(shopPrice(product({ display_price_label: '£2.50/kg' }))).toBeNull();
  });

  it('still rounds the float that a decimal price really is', () => {
    expect(
      shopPrice(
        product({
          display_price: { amount: 8.93, currencyCode: 'GBP' },
          display_price_label: '£8.93',
        }),
      )?.amount_minor,
    ).toBe(893);
  });
});

