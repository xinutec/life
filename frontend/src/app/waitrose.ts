import { Injectable } from '@angular/core';

/** Full detail for a Waitrose product (from the bridge's fetchProduct). Shape
 *  mirrors what the native WaitroseBridge returns; maps onto LifeApi.importProduct. */
export interface WaitroseProduct {
  source: string;
  external_id: string;
  name: string | null;
  brand: string | null;
  barcodes: string[];
  image_url: string | null;
  display_price: { amount: number; currencyCode: string } | null;
  categories: string[];
}

/** A lightweight search hit (from searchByName); fetchProduct() gets the rest. */
export interface WaitroseCandidate {
  lineNumber: string;
  name: string;
  image_url: string;
}

/** The native interface injected by the Android wrapper (absent in a browser). */
interface Bridge {
  available(): boolean;
  connect(): void;
  fetchProduct(lineNumber: string, requestId: string): void;
  searchByName(query: string, requestId: string): void;
}

type BridgeResult =
  | { ok: true; product?: WaitroseProduct; candidates?: WaitroseCandidate[] }
  | { ok: false; error: string };

interface BridgeWindow extends Window {
  WaitroseBridge?: Bridge;
  __waitroseResolve?: (requestId: string, result: BridgeResult) => void;
  __waitroseConnected?: () => void;
}

/**
 * Wraps the native WaitroseBridge, turning its callback-based methods into
 * Promises. The bridge runs a hidden WebView on waitrose.com to fetch product
 * data past Akamai — only possible inside the Life Android app, so `available`
 * is false in a plain browser and callers must feature-detect before offering
 * any Waitrose UI.
 */
@Injectable({ providedIn: 'root' })
export class Waitrose {
  private readonly win = window as BridgeWindow;
  private readonly bridge = this.win.WaitroseBridge;
  private readonly pending = new Map<string, (r: BridgeResult) => void>();
  private connectWaiters: (() => void)[] = [];

  constructor() {
    // The native side resolves a request by name; route it to the waiting call.
    this.win.__waitroseResolve = (requestId, result) => {
      const resolve = this.pending.get(requestId);
      if (resolve) {
        this.pending.delete(requestId);
        resolve(result);
      }
    };
    // Fired when the "Connect Waitrose" overlay closes.
    this.win.__waitroseConnected = () => {
      const waiters = this.connectWaiters;
      this.connectWaiters = [];
      waiters.forEach((fn) => fn());
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

  /** Show the Waitrose sign-in overlay; resolves when it closes. */
  connect(): Promise<void> {
    if (!this.bridge) return Promise.reject(new Error(UNAVAILABLE));
    return new Promise((resolve) => {
      this.connectWaiters.push(resolve);
      this.bridge!.connect();
    });
  }

  /** Search Waitrose by product name. */
  search(query: string): Promise<WaitroseCandidate[]> {
    return this.call((id) => this.bridge!.searchByName(query, id)).then((r) => r.candidates ?? []);
  }

  /** Fetch full detail for a Waitrose product by its lineNumber. */
  fetchProduct(lineNumber: string): Promise<WaitroseProduct> {
    return this.call((id) => this.bridge!.fetchProduct(lineNumber, id)).then((r) => {
      if (!r.product) throw new Error('no product returned');
      return r.product;
    });
  }

  private call(invoke: (requestId: string) => void): Promise<Extract<BridgeResult, { ok: true }>> {
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

const UNAVAILABLE = 'Waitrose is only available in the app';
