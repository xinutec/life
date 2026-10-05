import { Injectable } from '@angular/core';

import { PriceInput, Source } from './models';

/** A shop product as the WebView returns it. */
export interface ShopProduct {
  source: Source;
  external_id: string;
  name: string | null;
  brand: string | null;
  barcodes: string[];
  /** As the shop writes it ("100g"). */
  quantity_label: string | null;
  image_url: string | null;
  display_price: { amount: number; currencyCode: string } | null;
  /** The shop's own formatted price ("£2.50", "85p"), to check the number's
   *  unit against (see `shopPrice`). */
  display_price_label: string | null;
  categories: string[];
}

/** Pence from "£2.50" or "85p" (Waitrose writes both), or null. */
export function penceFromLabel(label: string): number | null {
  const pounds = /^£\s*(\d+)(?:\.(\d{1,2}))?$/.exec(label.trim());
  if (pounds) {
    const frac = (pounds[2] ?? '').padEnd(2, '0');
    return Number(pounds[1]) * 100 + Number(frac);
  }
  const pence = /^(\d{1,2})\s*p$/.exec(label.trim());
  return pence ? Number(pence[1]) : null;
}

/** A shop's quote in minor units, or null unless its formatted price agrees:
 *  `amount` does not say pounds or pence, and a 100x error looks like a price. */
export function shopPrice(product: ShopProduct): PriceInput | null {
  const p = product.display_price;
  if (!p || !(p.amount > 0)) return null;
  const minor = Math.round(p.amount * 100);
  const label = product.display_price_label;
  const shown = label === null ? null : penceFromLabel(label);
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
    unit_price: null,
  };
}

/** A search hit; fetchProduct() gets the rest. */
export interface ShopCandidate {
  external_id: string;
  name: string;
  image_url: string;
}

/** A product page's facts blob, carried past the bot wall for the server to
 *  parse. */
export interface ShopFacts {
  /** The page's own barcode, checked by the server. */
  ean: string;
  blob: string;
}

/** A shop whose page has facts its API lacks. Asda's search runs server-side,
 *  so this is its only WebView role. */
export interface FactsProvider {
  readonly id: Source;
  facts(externalId: string): { url: string; js: string };
}

/** A shop's page URLs and extractor JS for the hidden WebView, kept in the web
 *  app so a new shop needs no APK. */
export interface ShopProvider {
  readonly id: Source;
  readonly displayName: string;
  readonly loginUrl: string;
  search(query: string): { url: string; js: string };
  product(externalId: string): { url: string; js: string };
}

/** The Android wrapper's port; answers come back through `window.__shopResolve`. */
interface Bridge {
  postMessage(message: string): void;
}

type BridgeRequest =
  | { op: 'run'; url: string; extractorJs: string; requestId: string }
  | { op: 'connect'; loginUrl: string; requestId: string };

type BridgeResult =
  | { ok: true; product?: ShopProduct; candidates?: ShopCandidate[]; facts?: ShopFacts }
  | { ok: false; error: string; reason?: 'signed_out' };

/** The shop's session is signed out; `connect` fixes it. */
export class ShopSignedOut extends Error {}

export function isSignedOut(e: unknown): boolean {
  return e instanceof ShopSignedOut;
}

interface BridgeWindow extends Window {
  ShopBridge?: Bridge;
  __shopResolve?: (requestId: string, result: BridgeResult) => void;
  __shopConnected?: (requestId: string | null) => void;
}

/** The native ShopBridge as Promises. Only inside the Android app; check
 *  `available` before offering shop UI. */
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

  /** An older app's port lacks `postMessage` and reads as absent. */
  get available(): boolean {
    return typeof this.bridge?.postMessage === 'function';
  }

  connect(provider: ShopProvider): Promise<void> {
    if (!this.available) return Promise.reject(new Error(UNAVAILABLE));
    return this.request((requestId) =>
      this.send({ op: 'connect', loginUrl: provider.loginUrl, requestId }),
    ).then(() => undefined);
  }

  search(provider: ShopProvider, query: string): Promise<ShopCandidate[]> {
    const { url, js } = provider.search(query);
    return this.run(url, js).then((r) => r.candidates ?? []);
  }

  fetchProduct(provider: ShopProvider, externalId: string): Promise<ShopProduct> {
    const { url, js } = provider.product(externalId);
    return this.run(url, js).then((r) => {
      if (!r.product) throw new Error('no product returned');
      return r.product;
    });
  }

  fetchFacts(provider: FactsProvider, externalId: string): Promise<ShopFacts> {
    const { url, js } = provider.facts(externalId);
    return this.run(url, js).then((r) => {
      if (!r.facts) throw new Error('no facts returned');
      return r.facts;
    });
  }

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

  private request(
    invoke: (requestId: string) => void,
  ): Promise<Extract<BridgeResult, { ok: true }>> {
    if (!this.available) return Promise.reject(new Error(UNAVAILABLE));
    const requestId = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, (result) => {
        if (result.ok) resolve(result);
        else if (result.reason === 'signed_out') reject(new ShopSignedOut(result.error));
        else reject(new Error(result.error));
      });
      invoke(requestId);
    });
  }
}

const UNAVAILABLE = 'Shop enrichment is only available in the app';
