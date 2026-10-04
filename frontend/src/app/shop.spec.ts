import { afterEach, describe, expect, it, vi } from 'vitest';

import { PriceInput } from './models';
import { ShopProduct, ShopProvider, Shops, isSignedOut, penceFromLabel, shopPrice } from './shop';
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

/** The native side is an origin-scoped message port. Requests arrive as JSON;
 *  results come back through `window.__shopResolve`, because a hidden WebView's
 *  answer lands long after the message that asked for it. */
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

  it('tells a signed-out shop apart from any other failure', async () => {
    fakeBridge();
    const svc = new Shops();
    const out = svc.fetchProduct(provider, '1');
    w.__shopResolve!(lastRun!.id, { ok: false, error: 'no token', reason: 'signed_out' });
    const other = svc.fetchProduct(provider, '2');
    w.__shopResolve!(lastRun!.id, { ok: false, error: 'load failed' });
    expect(isSignedOut(await out.catch((e: unknown) => e))).toBe(true);
    expect(isSignedOut(await other.catch((e: unknown) => e))).toBe(false);
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
  it('searches waitrose.com for the term', () => {
    const { url } = WAITROSE.search('cheddar');
    expect(url).toContain('waitrose.com/ecom/shop/search?searchTerm=cheddar');
  });

  it('refuses a lineNumber that is not one, since it is spliced into the script', () => {
    expect(() => WAITROSE.product('not-a-number')).toThrow(/invalid/);
  });
});

/** Waitrose's own pricing blocks for two basmati rices: one on a "New Lower
 *  Price" offer, one not. */
const ON_OFFER = {
  displayPrice: '£1.80',
  promotions: [
    {
      promotionDescription: 'New Lower Price',
      promotionType: 'NLP',
      promotionUnitPrice: { amount: 1.8, currencyCode: 'GBP' },
      wasDisplayPrice: '£2.50',
      groups: [{ threshold: 1, name: 'X' }],
    },
  ],
  currentSaleUnitRetailPrice: { price: { amount: 2.5, currencyCode: 'GBP' } },
};
const REGULAR = {
  displayPrice: '£1.85',
  promotions: [],
  currentSaleUnitRetailPrice: { price: { amount: 1.85, currencyCode: 'GBP' } },
};

interface Run {
  report: { ok: boolean; reason?: string; error?: string; product?: ShopProduct };
  fetched?: { url: string; authorization: string | undefined };
}

/** Run the real product extractor as the WebView would, against one SUMMARY
 *  response. `token: null` is a signed-out page, which mints none. */
async function runProduct(
  product: Record<string, unknown>,
  token: string | null = 'Bearer t',
): Promise<Run> {
  const button = document.createElement('button');
  button.className = 'acceptAll';
  document.body.appendChild(button);
  const w = window as unknown as Record<string, unknown>;
  if (token === null) delete w['__authToken'];
  else w['__authToken'] = token;
  let reported = '';
  let fetched: Run['fetched'];
  vi.stubGlobal('AndroidShop', { result: (json: string) => (reported = json) });
  vi.stubGlobal('fetch', (url: string, init: { headers: Record<string, string> }) => {
    fetched = { url, authorization: init.headers['authorization'] };
    return Promise.resolve({
      status: 200,
      json: () =>
        Promise.resolve({
          products: [{ lineNumber: '504251', name: 'Rice', barCodes: [], weights: {}, ...product }],
        }),
    });
  });
  vi.useFakeTimers();
  try {
    const source = `return ${WAITROSE.product('504251').js.trim()}`;
    // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the extractor is JS text, run as the WebView runs it
    const run = new Function(source) as () => Promise<void>;
    const done = run();
    await vi.runAllTimersAsync();
    await done;
  } finally {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    button.remove();
  }
  return { report: JSON.parse(reported) as Run['report'], fetched };
}

/** The product the extractor reported for one pricing block. */
async function extract(pricing: unknown): Promise<ShopProduct> {
  const { report } = await runProduct({ pricing });
  return report.product!;
}

describe('Waitrose product extractor', () => {
  it('asks the SUMMARY API for that line, with the page’s token', async () => {
    const { fetched } = await runProduct({ pricing: REGULAR });
    expect(fetched?.url).toContain('/products-prod/v1/products/504251?view=SUMMARY');
    expect(fetched?.authorization).toBe('Bearer t');
  });

  it('reads the pack size off the weights block', async () => {
    // Waitrose states it on `weights.sizeDescription` ("42g"), not beside the name.
    const { report } = await runProduct({ pricing: REGULAR, weights: { sizeDescription: '42g' } });
    expect(report.product?.quantity_label).toBe('42g');
  });

  it('carries the formatted price, which is what the amount is checked against', async () => {
    const { report } = await runProduct({ pricing: REGULAR });
    expect(report.product?.display_price_label).toBe('£1.85');
  });

  it('names the sign-in when the page minted no token', async () => {
    // Signed out, Waitrose mints no Authorization header at all, so a bare
    // failure would read as a broken extractor and send the reader to the JS.
    const { report, fetched } = await runProduct({ pricing: REGULAR }, null);
    expect(report).toMatchObject({ ok: false, reason: 'signed_out' });
    expect(report.error).toContain('signed out of waitrose.com');
    expect(fetched).toBeUndefined();
  });
});

describe('Waitrose product price', () => {
  it('is the offer price when one item is on offer', async () => {
    // Waitrose then reports the regular price as the "current sale" price, and
    // the guard would refuse it against the £1.80 it displays.
    const p = await extract(ON_OFFER);
    expect(p.display_price).toEqual({ amount: 1.8, currencyCode: 'GBP' });
    expect(shopPrice(p)?.amount_minor).toBe(180);
  });

  it('is the sale price otherwise', async () => {
    const p = await extract(REGULAR);
    expect(shopPrice(p)?.amount_minor).toBe(185);
  });

  it('ignores a multi-buy, which is no price for one item', async () => {
    const multiBuy = {
      ...REGULAR,
      promotions: [
        { promotionUnitPrice: { amount: 1.5, currencyCode: 'GBP' }, groups: [{ threshold: 2 }] },
      ],
    };
    const p = await extract(multiBuy);
    expect(shopPrice(p)?.amount_minor).toBe(185);
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
      unit_price: null,
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
