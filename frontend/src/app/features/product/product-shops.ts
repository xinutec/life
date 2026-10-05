import { Signal, computed, inject, signal } from '@angular/core';

import { LifeApi } from '../../life-api';
import { ProductDetail, SeenListing, Source } from '../../models';
import { onlineHint } from '../../shared/api-error';
import { canonicalBarcode } from '../../shared/barcode';
import { Feedback } from '../../shared/feedback';
import { formatMoney } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { ShopProduct, ShopProvider, Shops, isSignedOut, shopPrice } from '../../shop';
import { WAITROSE } from '../../shops/waitrose';

/** `none`: checked, and not carried. `unknown`: nobody has looked and this
 *  device cannot. */
type ShopLookup = 'idle' | 'searching' | 'found' | 'none' | 'unknown' | 'signedOut' | 'error';

const FINDABLE_SOURCES: Source[] = ['asda', 'waitrose'];

const BRIDGE_PROVIDERS: ShopProvider[] = [WAITROSE];

function bridgeProvider(source: Source): ShopProvider | undefined {
  return BRIDGE_PROVIDERS.find((p) => p.id === source);
}

/** A shop hit, whoever found it. `product` is set when the phone fetched the
 *  record, so adding it costs no second page load. */
interface ShopHit {
  source: Source;
  external_id: string;
  name: string;
  brand: string | null;
  quantity_label: string | null;
  image_url: string | null;
  price_label: string | null;
  product: ShopProduct | null;
}

interface ShopLookupRow {
  source: Source;
  label: string;
  state: ShopLookup;
  /** "2 of 8": each step is a page load, and silence would read as a hang. */
  progress: string | null;
  hit: ShopHit | null;
  fromCache: boolean;
  /** Results checked: `none` means none of these. */
  checked: number;
}

function blankLookup(source: Source): ShopLookupRow {
  return {
    source,
    label: sourceLabel(source),
    state: 'idle',
    progress: null,
    hit: null,
    fromCache: false,
    checked: 0,
  };
}

function priceLabel(product: ShopProduct): string | null {
  const price = shopPrice(product);
  return price ? formatMoney(price.amount_minor, price.currency) : null;
}

/** Finding the product at shops and refreshing listed ones. The server answers
 *  from memory or searches Asda; Waitrose needs the app's WebView. */
export class ProductShops {
  private api = inject(LifeApi);
  private feedback = inject(Feedback);
  private shops = inject(Shops);

  constructor(
    private readonly id: () => number,
    private readonly detail: Signal<ProductDetail | null>,
    private readonly reload: () => void,
  ) {}

  private readonly lookupState = signal<Record<string, ShopLookupRow>>({});
  readonly attaching = signal(false);

  /** Shops not yet listed, if there is a barcode to match on. */
  readonly shopLookups = computed<ShopLookupRow[]>(() => {
    const d = this.detail();
    if (!d?.product.barcode) return [];
    const state = this.lookupState();
    return FINDABLE_SOURCES.filter((s) => !d.listings.some((l) => l.source === s)).map(
      (s) => state[s] ?? blankLookup(s),
    );
  });

  private patchLookup(source: Source, patch: Partial<ShopLookupRow>): void {
    const state = this.lookupState();
    this.lookupState.set({
      ...state,
      [source]: { ...(state[source] ?? blankLookup(source)), ...patch },
    });
  }

  /** Server first; `searched: false` hands over to the WebView hunt. */
  find(source: Source): void {
    this.patchLookup(source, { state: 'searching', hit: null, progress: null, checked: 0 });
    this.api.findAtShop(this.id(), source).subscribe({
      next: (found) => {
        if (found.hit) {
          this.patchLookup(source, {
            state: 'found',
            hit: { ...found.hit, source, product: null },
            fromCache: found.from_cache,
          });
        } else if (found.searched) {
          this.patchLookup(source, { state: 'none' });
        } else {
          void this.hunt(source);
        }
      },
      error: () => this.patchLookup(source, { state: 'error' }),
    });
  }

  /** Open each search result until one carries our barcode. Every page read is
   *  reported, so no hunt pays for it twice. */
  private async hunt(source: Source): Promise<void> {
    const provider = bridgeProvider(source);
    const d = this.detail();
    const barcode = d?.product.barcode;
    const name = d?.product.name?.trim();
    if (!provider || !this.shops.available || !barcode || !name) {
      // Say nobody looked, rather than "not carried".
      this.patchLookup(source, { state: 'unknown', progress: null });
      return;
    }
    try {
      const candidates = await this.shops.search(provider, name);
      this.remember(
        source,
        candidates.map((c) => ({
          external_id: c.external_id,
          name: c.name,
          image_url: c.image_url,
          barcode: null,
          brand: null,
          quantity_label: null,
        })),
      );
      for (const [i, candidate] of candidates.entries()) {
        this.patchLookup(source, {
          progress: `${i + 1} of ${candidates.length}`,
          checked: i,
        });
        const product = await this.shops.fetchProduct(provider, candidate.external_id);
        const codes = product.barcodes.map(canonicalBarcode);
        const matched = codes.includes(barcode);
        this.remember(source, [
          {
            external_id: product.external_id,
            // File the matched page under our barcode, so the next lookup hits.
            barcode: matched ? barcode : (codes[0] ?? null),
            name: product.name,
            brand: product.brand,
            image_url: product.image_url,
            quantity_label: product.quantity_label,
          },
        ]);
        if (matched) {
          this.patchLookup(source, {
            state: 'found',
            progress: null,
            checked: i + 1,
            fromCache: false,
            hit: {
              source,
              external_id: product.external_id,
              name: product.name ?? candidate.name,
              brand: product.brand,
              quantity_label: product.quantity_label,
              image_url: product.image_url,
              price_label: priceLabel(product),
              product,
            },
          });
          return;
        }
      }
      this.patchLookup(source, { state: 'none', progress: null, checked: candidates.length });
    } catch (e: unknown) {
      this.patchLookup(source, { state: isSignedOut(e) ? 'signedOut' : 'error', progress: null });
    }
  }

  signIn(source: Source): void {
    const provider = bridgeProvider(source);
    if (!provider) return;
    this.shops.connect(provider).then(
      () => this.find(source),
      () => this.feedback.error(`Could not open the ${provider.displayName} sign-in.`),
    );
  }

  /** Best-effort, but logged: a cache that never writes looks like one that works. */
  private remember(source: Source, listings: SeenListing[]): void {
    if (!listings.length) return;
    this.api.rememberShopListings(source, listings).subscribe({
      error: (e: unknown) => console.warn(`[shop:${source}] could not remember what we saw`, e),
    });
  }

  hitSubtitle(hit: ShopHit): string {
    return [hit.brand, hit.quantity_label].filter((s) => !!s).join(' · ');
  }

  /** Asda is re-read and re-checked by the server; a WebView shop is imported
   *  from the record the phone fetched. */
  attachHit(row: ShopLookupRow): void {
    const hit = row.hit;
    if (!hit || this.attaching()) return;
    if (hit.source === 'asda') {
      this.pull(hit.external_id, `Added ${row.label}.`, `Could not add ${row.label}`);
      return;
    }
    const barcode = this.detail()?.product.barcode ?? null;
    this.importListing(
      hit.source,
      {
        source: hit.source,
        external_id: hit.external_id,
        name: hit.name,
        brand: hit.brand,
        quantity_label: hit.quantity_label,
        barcode,
        image_url: hit.image_url,
        price: hit.product ? shopPrice(hit.product) : null,
      },
      `Added ${row.label}.`,
      `Could not add ${row.label}`,
    );
  }

  canRefresh(source: Source): boolean {
    return source === 'asda' || (this.shops.available && !!bridgeProvider(source));
  }

  /** On demand only: a price you did not ask for is worse than an old one. */
  refresh(row: { source: Source; externalId: string; label: string }): void {
    if (row.source === 'asda') {
      this.pull(row.externalId, `Refreshed ${row.label}.`, `Could not refresh ${row.label}`);
      return;
    }
    const provider = bridgeProvider(row.source);
    if (!provider || !this.shops.available || this.attaching()) return;
    this.attaching.set(true);
    this.shops
      .fetchProduct(provider, row.externalId)
      .then((product) => {
        this.attaching.set(false);
        this.importListing(
          row.source,
          {
            source: row.source,
            external_id: product.external_id,
            name: product.name ?? row.label,
            brand: product.brand,
            quantity_label: product.quantity_label,
            barcode: this.detail()?.product.barcode ?? null,
            image_url: product.image_url,
            price: shopPrice(product),
          },
          `Refreshed ${row.label}.`,
          `Could not refresh ${row.label}`,
        );
      })
      .catch(() => {
        this.attaching.set(false);
        this.feedback.error(`Could not refresh ${row.label} — is the app signed in?`);
      });
  }

  private importListing(
    source: Source,
    body: Parameters<LifeApi['importProduct']>[0],
    ok: string,
    bad: string,
  ): void {
    if (this.attaching()) return;
    this.attaching.set(true);
    this.api.importProduct(body).subscribe({
      next: () => {
        this.attaching.set(false);
        this.patchLookup(source, blankLookup(source));
        this.feedback.notify(ok);
        this.reload();
      },
      error: (e: unknown) => {
        this.attaching.set(false);
        this.feedback.error(`${bad}${onlineHint(e)}`);
      },
    });
  }

  private pull(externalId: string, ok: string, bad: string): void {
    if (this.attaching()) return;
    this.attaching.set(true);
    this.api.syncListing(this.id(), 'asda', externalId).subscribe({
      next: () => {
        this.attaching.set(false);
        this.patchLookup('asda', blankLookup('asda'));
        this.feedback.notify(ok);
        this.reload();
      },
      error: (e: unknown) => {
        this.attaching.set(false);
        this.feedback.error(`${bad}${onlineHint(e)}`);
      },
    });
  }
}
