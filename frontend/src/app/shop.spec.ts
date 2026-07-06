import { afterEach, describe, expect, it } from 'vitest';

import { ShopProvider, Shops } from './shop';
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

function fakeBridge() {
  lastRun = undefined;
  lastConnect = undefined;
  w.ShopBridge = {
    available: () => true,
    connect: (loginUrl: string, id: string) => (lastConnect = { loginUrl, id }),
    run: (url: string, js: string, id: string) => (lastRun = { url, js, id }),
  };
}

const provider: ShopProvider = {
  id: 'test',
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
});
