import { Injectable } from '@angular/core';

/** Full detail for a shop product; maps onto LifeApi.importProduct. */
export interface ShopProduct {
  source: string;
  external_id: string;
  name: string | null;
  brand: string | null;
  barcodes: string[];
  image_url: string | null;
  display_price: { amount: number; currencyCode: string } | null;
  categories: string[];
}

/** A lightweight search hit; fetchProduct() gets the rest. */
export interface ShopCandidate {
  external_id: string;
  name: string;
  image_url: string;
}

/** Raw product-page content a shop's WebView returns for the SERVER to parse:
 *  facts the shop's API doesn't carry (Asda's Brandbank nutrition/ingredients/
 *  allergens/dietary). The client never interprets it — it only ferries the blob
 *  past the bot-wall. */
export interface ShopFacts {
  /** The page's own barcode (Asda's c_EAN_GTIN) — the backend's identity guard. */
  ean: string;
  /** The raw product-content blob (Asda's c_BRANDBANK_JSON), parsed server-side. */
  blob: string;
}

/** A shop whose product PAGE carries facts its search API doesn't, readable only
 *  through the hidden WebView (the page is behind a bot-wall). Kept separate from
 *  ShopProvider: Asda's search/import already run server-side (see products::asda),
 *  so its WebView role is facts alone. */
export interface FactsProvider {
  readonly id: string; // matches products.source ('asda')
  /** Load this product's page and return its raw facts blob for the server. */
  facts(externalId: string): { url: string; js: string };
}

/**
 * Everything shop-specific — URLs, consent handling, result extraction — lives in
 * a provider, in the web app, so adding a shop (Asda) needs no APK change. Each
 * op returns the page URL to load plus the extractor JS to run in the hidden
 * WebView. That JS reads `window.__authToken` (any Bearer the page minted, captured
 * by the native layer) and reports `AndroidShop.result(JSON.stringify(...))`:
 * `{ ok, candidates }` for search, `{ ok, product }` for product, or `{ ok:false, error }`.
 */
export interface ShopProvider {
  readonly id: string; // matches products.source ('waitrose', 'asda')
  readonly displayName: string;
  readonly loginUrl: string; // shown by connect()
  search(query: string): { url: string; js: string };
  product(externalId: string): { url: string; js: string };
}

/** The native interface injected by the Android wrapper (absent in a browser). */
interface Bridge {
  available(): boolean;
  connect(loginUrl: string, requestId: string): void;
  run(url: string, extractorJs: string, requestId: string): void;
}

type BridgeResult =
  | { ok: true; product?: ShopProduct; candidates?: ShopCandidate[]; facts?: ShopFacts }
  | { ok: false; error: string };

interface BridgeWindow extends Window {
  ShopBridge?: Bridge;
  __shopResolve?: (requestId: string, result: BridgeResult) => void;
  __shopConnected?: (requestId: string | null) => void;
}

/**
 * Drives the native ShopBridge, turning its callback-based methods into Promises.
 * The bridge runs a hidden WebView on a shop site to fetch data past the shop's
 * bot-wall — only possible inside the Life Android app, so `available` is false
 * in a plain browser and callers must feature-detect before offering any shop UI.
 */
@Injectable({ providedIn: 'root' })
export class Shops {
  private readonly win = window as BridgeWindow;
  private readonly bridge = this.win.ShopBridge;
  private readonly pending = new Map<string, (r: BridgeResult) => void>();

  constructor() {
    this.win.__shopResolve = (requestId, result) => this.settle(requestId, result);
    this.win.__shopConnected = (requestId) => {
      if (requestId) this.settle(requestId, { ok: true });
    };
  }

  /** True only inside the Android app (the native bridge is present). */
  get available(): boolean {
    try {
      return !!this.bridge?.available();
    } catch {
      return false;
    }
  }

  /** Show the shop's sign-in overlay; resolves when it closes. */
  connect(provider: ShopProvider): Promise<void> {
    if (!this.bridge) return Promise.reject(new Error(UNAVAILABLE));
    return this.request((id) => this.bridge!.connect(provider.loginUrl, id)).then(() => undefined);
  }

  /** Search a shop by product name. */
  search(provider: ShopProvider, query: string): Promise<ShopCandidate[]> {
    const { url, js } = provider.search(query);
    return this.request((id) => this.bridge!.run(url, js, id)).then((r) => r.candidates ?? []);
  }

  /** Fetch full detail for a product by its shop external id. */
  fetchProduct(provider: ShopProvider, externalId: string): Promise<ShopProduct> {
    const { url, js } = provider.product(externalId);
    return this.request((id) => this.bridge!.run(url, js, id)).then((r) => {
      if (!r.product) throw new Error('no product returned');
      return r.product;
    });
  }

  /** Fetch a product page's raw facts blob through the WebView, for the server to
   *  parse. Only meaningful for a shop whose page is bot-walled (Asda). */
  fetchFacts(provider: FactsProvider, externalId: string): Promise<ShopFacts> {
    const { url, js } = provider.facts(externalId);
    return this.request((id) => this.bridge!.run(url, js, id)).then((r) => {
      if (!r.facts) throw new Error('no facts returned');
      return r.facts;
    });
  }

  private settle(requestId: string, result: BridgeResult): void {
    const resolve = this.pending.get(requestId);
    if (resolve) {
      this.pending.delete(requestId);
      resolve(result);
    }
  }

  private request(invoke: (requestId: string) => void): Promise<Extract<BridgeResult, { ok: true }>> {
    if (!this.bridge) return Promise.reject(new Error(UNAVAILABLE));
    const requestId = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, (result) => {
        if (result.ok) resolve(result);
        else reject(new Error(result.error));
      });
      invoke(requestId);
    });
  }
}

const UNAVAILABLE = 'Shop enrichment is only available in the app';
