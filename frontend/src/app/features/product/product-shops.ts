import { Signal, computed, inject, signal } from '@angular/core';

import { LifeApi } from '../../life-api';
import { ProductDetail, SeenListing, Source } from '../../models';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { formatMoney } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { ShopProduct, ShopProvider, Shops, isSignedOut, shopPrice } from '../../shop';
import { WAITROSE } from '../../shops/waitrose';

/** How a shop lookup is going.
 *
 *  `none` = the shop's own results were checked and none carried this barcode.
 *  `unknown` = nobody has ever looked, and this device can't: the shop is behind
 *  a bot-wall only the app's hidden WebView gets through. Saying "doesn't have
 *  it" for that would claim an answer nobody asked for.
 *  `signedOut` = the app's session with the shop is signed out, so it would not
 *  answer; `signIn` fixes that and looks again. */
type ShopLookup = 'idle' | 'searching' | 'found' | 'none' | 'unknown' | 'signedOut' | 'error';

/** The shops a product can be looked up at, in the order they're offered. Both
 *  are answerable from memory anywhere; only Asda can be searched afresh without
 *  the app (see [[find]]). */
const FINDABLE_SOURCES: Source[] = ['asda', 'waitrose'];

/** Shops the app's WebView can walk itself. Keyed by source id so a lookup, a
 *  refresh and the picker all reach the same provider. */
const BRIDGE_PROVIDERS: ShopProvider[] = [WAITROSE];

function bridgeProvider(source: Source): ShopProvider | undefined {
  return BRIDGE_PROVIDERS.find((p) => p.id === source);
}

/** A shop hit as this screen shows it, whoever found it: the server's own Asda
 *  search, our memory of an earlier query, or the phone walking a bot-walled
 *  shop. `product` is set only in the last case — it's the full record already in
 *  hand, so adding it costs no second page load. */
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

/** One shop's lookup as the screen renders it. */
interface ShopLookupRow {
  source: Source;
  label: string;
  state: ShopLookup;
  /** What a hunt is doing right now ("2 of 8") — each step is a page load in a
   *  hidden WebView, so ten silent seconds would read as a hang. */
  progress: string | null;
  hit: ShopHit | null;
  fromCache: boolean;
  /** How many of the shop's own results were checked. `none` means "none of
   *  these", not "not in the catalogue", and the copy has to be able to say so. */
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

/** What a bot-walled shop's own quote reads as before we've stored it — the same
 *  string the buy rows show, from the record the WebView just fetched. */
function priceLabel(product: ShopProduct): string | null {
  const price = shopPrice(product);
  return price ? formatMoney(price.amount_minor, price.currency) : null;
}

/** Finding a product at shops, attaching what is found, and refreshing listed
 *  shops. Owned by the product page and constructed in its injection context.
 *
 *  Memory answers first, in any browser. On a miss the server searches Asda
 *  itself; Waitrose's bot wall needs the app's WebView, so the phone searches
 *  and reports what it saw. */
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

  /** One lookup row per shop that could still be added: we need a barcode to
   *  match on, and there is nothing to find at a shop already listed. */
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

  /** Ask whether a shop carries this barcode.
   *
   *  The server answers from what past queries taught it, so a repeat lookup
   *  costs the shop nothing. On a miss it searches the shops it can reach and
   *  matches on the EAN — never on the shop's relevance order, which is no
   *  evidence of identity. When it can't reach the shop at all it says so
   *  (`searched: false`) rather than reporting an absence it never checked, and
   *  the hunt below takes over if this device can do the looking. */
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

  /** Walk a bot-walled shop's own search results in the app's hidden WebView
   *  until one of them carries our barcode.
   *
   *  Waitrose's search hits carry no EAN — only a product fetch does — so this
   *  is a page load per candidate, which is exactly why every one it passes over
   *  is reported to the backend on the way. The eight pages a fruitless hunt
   *  costs are then eight lookups nobody has to pay for again, for this product
   *  or any other. */
  private async hunt(source: Source): Promise<void> {
    const provider = bridgeProvider(source);
    const d = this.detail();
    const barcode = d?.product.barcode;
    const name = d?.product.name?.trim();
    if (!provider || !this.shops.available || !barcode || !name) {
      // Nobody has looked and this device can't. Say that, rather than letting
      // an unasked question read as a negative answer.
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
        const matched = product.barcodes.includes(barcode);
        this.remember(source, [
          {
            external_id: product.external_id,
            // A listing gets one row, so of several EANs we keep the one that
            // identifies it FOR US when there is one — otherwise the next
            // lookup for this barcode would miss a page we have already read.
            barcode: matched ? barcode : (product.barcodes[0] ?? null),
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

  /** Show the shop's sign-in, then look again once it closes. */
  signIn(source: Source): void {
    const provider = bridgeProvider(source);
    if (!provider) return;
    this.shops.connect(provider).then(
      () => this.find(source),
      () => this.feedback.error(`Could not open the ${provider.displayName} sign-in.`),
    );
  }

  /** File what the WebView saw. Best-effort by design: this is a side benefit of
   *  a lookup the user asked for, so a failed cache write must not fail their
   *  hunt — but it is reported, because a cache that silently never writes looks
   *  exactly like one that works. */
  private remember(source: Source, listings: SeenListing[]): void {
    if (!listings.length) return;
    this.api.rememberShopListings(source, listings).subscribe({
      error: (e: unknown) => console.warn(`[shop:${source}] could not remember what we saw`, e),
    });
  }

  /** "Brand · 400G" for a shop hit we haven't imported yet — the same subtitle
   *  shape as the product page, from whatever the shop handed back. Lets you
   *  size up the match before committing to Add. */
  hitSubtitle(hit: ShopHit): string {
    return [hit.brand, hit.quantity_label].filter((s) => !!s).join(' · ');
  }

  /** Add the found listing to this product.
   *
   *  Asda is re-read shop-side, so the match this screen made is a convenience
   *  the server never takes on trust — it re-checks the barcode itself. A
   *  bot-walled shop can't be re-read from there, so we import the record the
   *  phone already fetched; a hit that came from memory has no record and no
   *  price, and lands as a listing whose price a refresh fills in. */
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

  /** Whether a listed shop can be re-read from this device. Asda always (the
   *  server does it); the rest only in the app, whose WebView is the only thing
   *  that can see their pages. */
  canRefresh(source: Source): boolean {
    return source === 'asda' || (this.shops.available && !!bridgeProvider(source));
  }

  /** Re-read a shop listing on demand — pressed when you've seen the shelf price
   *  change. Nothing refetches on a timer: shop data goes stale silently, and a
   *  wrong price you didn't ask for is worse than an old one you can refresh. */
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
