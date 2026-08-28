import { Injectable } from '@angular/core';

import { PriceInput, Source } from './models';

/** Full detail for a shop product; maps onto LifeApi.importProduct. */
export interface ShopProduct {
  source: Source;
  external_id: string;
  name: string | null;
  brand: string | null;
  barcodes: string[];
  /** The pack the shop sells, as the shop writes it ("100g"). Read into an
   *  amount server-side (products::packsize), which is what lets stock linked
   *  from a shop pick start out knowing how much it holds. `null` when the
   *  shop's payload carries no size. */
  quantity_label: string | null;
  image_url: string | null;
  display_price: { amount: number; currencyCode: string } | null;
  /** The shop's OWN formatted price for the same quote ("£2.50", "85p"), kept
   *  beside the number so the number's unit can be checked rather than assumed —
   *  see `shopPrice`. `null` when the shop didn't render one. */
  display_price_label: string | null;
  categories: string[];
}

/** Pence read out of a shop's formatted price. `null` if it isn't a shape we
 *  recognise — a refusal to guess, not a parse failure to paper over.
 *
 *  Both UK shapes are here because Waitrose renders both: "£2.50" at or above a
 *  pound, bare "85p" below it. */
export function penceFromLabel(label: string): number | null {
  const pounds = /^£\s*(\d+)(?:\.(\d{1,2}))?$/.exec(label.trim());
  if (pounds) {
    const frac = (pounds[2] ?? '').padEnd(2, '0');
    return Number(pounds[1]) * 100 + Number(frac);
  }
  const pence = /^(\d{1,2})\s*p$/.exec(label.trim());
  return pence ? Number(pence[1]) : null;
}

/** What a shop's own quote becomes on our side: integer minor units, never a
 *  float (see products::prices). `null` when the shop quoted nothing — and also
 *  when it quoted something whose unit we can't confirm.
 *
 *  **Why the confirmation.** `amount` is a bare number: nothing in the payload
 *  says whether it means pounds or pence, and reading it wrong stores money off
 *  by 100×. That error is a quiet one — £2.50 filed as £250 still looks like a
 *  price, and only surfaces later as "which shop is cheaper" answering wrongly.
 *  So the shop's own formatted price is the second opinion. If the two disagree,
 *  or there is no label to check against, nothing is recorded: a missing price
 *  is visibly missing, a wrong one isn't.
 *
 *  This beats confirming the unit by hand once, because it keeps holding if the
 *  shop ever changes it.
 *
 *  Shared rather than repeated: the picker imports shop products and so does the
 *  product page's shop lookup, and a price recorded by one path but not the
 *  other would make "which shop is cheaper" depend on which screen you used. */
export function shopPrice(product: ShopProduct): PriceInput | null {
  const p = product.display_price;
  if (!p || !(p.amount > 0)) return null;
  const minor = Math.round(p.amount * 100);
  const label = product.display_price_label;
  const shown = label === null ? null : penceFromLabel(label);
  // Dropped rather than stored, and logged rather than dropped silently — the
  // same rule the shop-cache report follows for an image URL it won't trust.
  if (shown === null) {
    console.warn(
      `[shop:price] ${product.external_id}: no formatted price to check ${minor}p against; not recording`,
      label,
    );
    return null;
  }
  if (shown !== minor) {
    console.warn(
      `[shop:price] ${product.external_id}: the shop shows ${label} (${shown}p) but its amount reads ${minor}p; not recording`,
    );
    return null;
  }
  return {
    amount_minor: minor,
    currency: p.currencyCode,
    // The SUMMARY payload's per-unit price sits behind a different view; until
    // we read it, saying nothing beats guessing a measure.
    unit_amount_minor: null,
    unit_measure: null,
    // One price, no nations — unlike Asda, which quotes EN/NI/SC/WA separately.
    region: null,
  };
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
  /** The source this provider speaks for — the same closed set the backend
   *  stores, so a provider can't name a shop the server doesn't know. */
  readonly id: Source;
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
  /** The source this provider speaks for (see FactsProvider.id). */
  readonly id: Source;
  readonly displayName: string;
  readonly loginUrl: string; // shown by connect()
  search(query: string): { url: string; js: string };
  product(externalId: string): { url: string; js: string };
}

/**
 * The native port injected by the Android wrapper (absent in a browser).
 *
 * A message port rather than the three plain methods it used to be. The wrapper
 * injects it with `WebViewCompat.addWebMessageListener`, whose origin rules keep
 * it out of any frame that isn't this app — the API it replaced was injected into
 * every frame in the WebView, including iframes, and this bridge takes a URL
 * *and JavaScript to run against it*.
 *
 * Only the outbound direction changed: results still arrive through
 * `window.__shopResolve` / `__shopConnected`, because a hidden WebView's answer
 * comes back long after the message that asked for it.
 */
interface Bridge {
  postMessage(message: string): void;
}

/** What the native side will do for us. */
type BridgeRequest =
  | { op: 'run'; url: string; extractorJs: string; requestId: string }
  | { op: 'connect'; loginUrl: string; requestId: string };

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

  /** True only inside the Android app.
   *
   *  The port's presence *is* the answer now: the wrapper only injects it for
   *  this app's own origin, so there is nothing for an `available()` call to
   *  establish that being able to ask hasn't already established. An app older
   *  than this page injects the previous shape, which has no `postMessage` — that
   *  reads as no bridge, and shop features are quietly absent until it's updated,
   *  rather than throwing on the first call. */
  get available(): boolean {
    return typeof this.bridge?.postMessage === 'function';
  }

  /** Show the shop's sign-in overlay; resolves when it closes. */
  connect(provider: ShopProvider): Promise<void> {
    if (!this.available) return Promise.reject(new Error(UNAVAILABLE));
    return this.request((requestId) =>
      this.send({ op: 'connect', loginUrl: provider.loginUrl, requestId }),
    ).then(() => undefined);
  }

  /** Search a shop by product name. */
  search(provider: ShopProvider, query: string): Promise<ShopCandidate[]> {
    const { url, js } = provider.search(query);
    return this.run(url, js).then((r) => r.candidates ?? []);
  }

  /** Fetch full detail for a product by its shop external id. */
  fetchProduct(provider: ShopProvider, externalId: string): Promise<ShopProduct> {
    const { url, js } = provider.product(externalId);
    return this.run(url, js).then((r) => {
      if (!r.product) throw new Error('no product returned');
      return r.product;
    });
  }

  /** Fetch a product page's raw facts blob through the WebView, for the server to
   *  parse. Only meaningful for a shop whose page is bot-walled (Asda). */
  fetchFacts(provider: FactsProvider, externalId: string): Promise<ShopFacts> {
    const { url, js } = provider.facts(externalId);
    return this.run(url, js).then((r) => {
      if (!r.facts) throw new Error('no facts returned');
      return r.facts;
    });
  }

  /** Load `url` in the hidden WebView and run `extractorJs` there. */
  private run(url: string, extractorJs: string): Promise<Extract<BridgeResult, { ok: true }>> {
    return this.request((requestId) => this.send({ op: 'run', url, extractorJs, requestId }));
  }

  private send(request: BridgeRequest): void {
    this.bridge?.postMessage(JSON.stringify(request));
  }

  private settle(requestId: string, result: BridgeResult): void {
    const resolve = this.pending.get(requestId);
    if (resolve) {
      this.pending.delete(requestId);
      resolve(result);
    }
  }

  private request(invoke: (requestId: string) => void): Promise<Extract<BridgeResult, { ok: true }>> {
    if (!this.available) return Promise.reject(new Error(UNAVAILABLE));
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
