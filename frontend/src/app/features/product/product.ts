import { Location } from '@angular/common';
import { Component, computed, effect, inject, input, numberAttribute, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatRadioModule } from '@angular/material/radio';

import { LifeApi } from '../../life-api';
import {
  Choice,
  Claim,
  FieldChoice,
  ProductDetail,
  ProductListing,
  ReconcileField,
  SeenListing,
  Source,
} from '../../models';
import { ago } from '../../shared/ago';
import { assertNever, classifyApiError, onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { fromMinorUnits } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { ShopProduct, ShopProvider, Shops, shopPrice } from '../../shop';
import { ASDA_FACTS } from '../../shops/asda';
import { WAITROSE } from '../../shops/waitrose';

/** One "where to buy" line: a shop that lists the product, with its current
 *  price (when one has been observed) and a deep link to its product page.
 *  `key` is the listing's identity — a shop can appear once, but the label is
 *  a display string and must never be used to identify a row. */
interface BuyRow {
  key: string;
  label: string;
  /** The source's own id for the listing — what a refresh re-reads. */
  externalId: string;
  source: Source;
  url: string | null;
  price: string | null;
  perUnit: string | null;
  observed: string | null;
}

/** One line of the nutrition table. `sub` marks the "of which …" rows. */
interface NutrientRow {
  label: string;
  value: string;
  sub: boolean;
}

/** A dietary chip: the flag humanized, styled by its tri-state value. The value
 *  is the wire's own `Claim` — the backend types it as an enum, so there is no
 *  re-declaration here to drift from it and nothing to assert. */
interface DietaryChip {
  label: string;
  value: Claim;
}

/** One safety-critical fact the sources disagree about, with each source's own
 *  word for it — shown as provenance (the safe merge still governs what's
 *  displayed above; this is so you can see the disagreement and check the label). */
interface FactConflict {
  label: string;
  perSource: { source: string; value: string }[];
}

/** How a shop lookup is going.
 *
 *  `none` = the shop's own results were checked and none carried this barcode.
 *  `unknown` = nobody has ever looked, and this device can't: the shop is behind
 *  a bot-wall only the app's hidden WebView gets through. The two were one state
 *  while Asda was the only shop, which would have made this screen say "Waitrose
 *  doesn't have it" when the truth was "we never asked". */
type ShopLookup = 'idle' | 'searching' | 'found' | 'none' | 'unknown' | 'error';

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


/** A listing's identity — what joins a price to the listing that quoted it, and
 *  what keys a row. `(source, external_id)` is the listing's unique key. */
function listingKey(l: { source: Source; external_id: string }): string {
  return `${l.source}/${l.external_id}`;
}

/** "gluten_free" → "Gluten free". */
function humanize(slug: string): string {
  const words = slug.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Minor units → a display amount ("£3.57"; non-GBP falls back to "3.57 EUR"). */
function money(amountMinor: number, currency: string): string {
  const amount = (amountMinor / 100).toFixed(2);
  return currency === 'GBP' ? `£${amount}` : `${amount} ${currency}`;
}

/** What a bot-walled shop's own quote reads as before we've stored it — the same
 *  string the buy rows show, from the record the WebView just fetched. */
function priceLabel(product: ShopProduct): string | null {
  const price = shopPrice(product);
  return price ? money(price.amount_minor, price.currency) : null;
}

/** The product payoff screen (/product/:id): hero image, clean name, where to
 *  buy at what price (deep links into the shops), the nutrition panel,
 *  ingredients, and allergen/dietary chips — everything the data model knows,
 *  one screen. Reached from an item's sheet ("View product") and the shell's
 *  "Scan a product". */
@Component({
  selector: 'app-product-page',
  templateUrl: './product.html',
  styleUrl: './product.scss',
  imports: [
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatRadioModule,
    ListState,
  ],
})
export class ProductPage {
  /** The routed product id. Route params arrive as strings (see
   *  withComponentInputBinding); `numberAttribute` is the one place that
   *  conversion happens — a junk id becomes NaN and is caught in `load`. */
  readonly id = input.required({ transform: numberAttribute });

  private api = inject(LifeApi);
  private location = inject(Location);
  private feedback = inject(Feedback);
  private shops = inject(Shops);

  readonly detail = signal<ProductDetail | null>(null);
  readonly loading = signal(true);
  readonly error = signal(false);
  /** Why the load failed, in the user's terms. Never "are you online?" for a
   *  product that simply isn't there — see shared/api-error. */
  readonly errorText = signal('');

  constructor() {
    effect(() => this.load(this.id()));
  }

  private load(id: number): void {
    this.detail.set(null);
    if (!Number.isFinite(id)) {
      // A hand-typed or stale URL. Say so instead of asking the server about it.
      this.loading.set(false);
      this.fail('That product link isn’t valid.');
      return;
    }
    this.loading.set(true);
    this.error.set(false);
    this.api.getProductDetail(id).subscribe({
      next: (d) => {
        this.detail.set(d);
        this.loading.set(false);
      },
      error: (e: unknown) => {
        this.loading.set(false);
        const f = classifyApiError(e);
        switch (f.kind) {
          case 'offline':
            this.fail('Can’t reach the server — you appear to be offline.');
            break;
          case 'unauthenticated':
            this.fail('Your session has expired — sign in again.');
            break;
          case 'server':
            this.fail(
              f.status === 404
                ? 'That product isn’t in the catalogue.'
                : 'The server couldn’t load this product.',
            );
            break;
          default:
            assertNever(f);
        }
      },
    });
  }

  private fail(message: string): void {
    this.errorText.set(message);
    this.error.set(true);
  }

  reload(): void {
    this.load(this.id());
  }

  // --- Finding this product at a shop ---
  //
  // Two halves, because the shops differ in who can see them, not in what a
  // sighting means. Asda's storefront search is a public API the server calls
  // from anywhere; Waitrose is behind a bot-wall only the app's hidden WebView
  // passes, so the phone does that looking and reports what it saw. Both are
  // answered from memory first, in any browser — a lookup someone already paid
  // for is free forever after, and that half needs no app at all.

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
    this.lookupState.set({ ...state, [source]: { ...(state[source] ?? blankLookup(source)), ...patch } });
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
            quantity_label: null,
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
              quantity_label: null,
              image_url: product.image_url,
              price_label: priceLabel(product),
              product,
            },
          });
          return;
        }
      }
      this.patchLookup(source, { state: 'none', progress: null, checked: candidates.length });
    } catch {
      this.patchLookup(source, { state: 'error', progress: null });
    }
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
  refresh(row: BuyRow): void {
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

  back(): void {
    this.location.back();
  }

  // --- Our own name: a hand correction when every source is wrong ---

  readonly editingName = signal(false);
  readonly nameDraft = signal('');

  startEditName(): void {
    this.nameDraft.set(this.detail()?.product.name ?? '');
    this.editingName.set(true);
  }

  cancelEditName(): void {
    this.editingName.set(false);
  }

  /** Save our own name — a `user`-owned value that outranks every source and is
   *  never auto-overwritten. Routed through reconcile so it settles the name
   *  divergence in the same step (the shops still keep their own spelling). */
  saveName(): void {
    const value = this.nameDraft().trim();
    if (!value || this.reconciling()) return;
    this.reconciling.set(true);
    this.api.reconcile(this.id(), [{ field: 'name', choice: 'user', value }]).subscribe({
      next: (d) => {
        this.reconciling.set(false);
        this.editingName.set(false);
        this.detail.set(d);
        this.feedback.notify('Renamed.');
      },
      error: (e: unknown) => {
        this.reconciling.set(false);
        this.feedback.error(`Could not rename${onlineHint(e)}`);
      },
    });
  }

  // --- Our own brand + pack size: hand corrections, same as the name ---
  //
  // The pack size is where this started: a shop's own casing ("250ML") that no
  // source disagrees with, so the picker can't fix it — only our own layer can.
  // Brand rides along for the same reason.

  readonly editingDetails = signal(false);
  readonly brandDraft = signal('');
  readonly packDraft = signal('');

  startEditDetails(): void {
    const p = this.detail()?.product;
    this.brandDraft.set(p?.brand ?? '');
    this.packDraft.set(p?.quantity_label ?? '');
    this.editingDetails.set(true);
  }

  cancelEditDetails(): void {
    this.editingDetails.set(false);
  }

  /** Save our own brand/pack — `user`-owned values (like the name) that outrank
   *  the sources and survive a refresh. Only fields you actually changed to a
   *  non-empty value are sent; an unchanged or emptied field is left alone (this
   *  path corrects, it doesn't clear). Nothing changed → just close. */
  saveDetails(): void {
    if (this.reconciling()) return;
    const p = this.detail()?.product;
    const decisions: FieldChoice[] = [];
    const brand = this.brandDraft().trim();
    if (brand && brand !== (p?.brand ?? '')) {
      decisions.push({ field: 'brand', choice: 'user', value: brand });
    }
    const pack = this.packDraft().trim();
    if (pack && pack !== (p?.quantity_label ?? '')) {
      decisions.push({ field: 'quantity_label', choice: 'user', value: pack });
    }
    if (!decisions.length) {
      this.editingDetails.set(false);
      return;
    }
    this.reconciling.set(true);
    this.api.reconcile(this.id(), decisions).subscribe({
      next: (d) => {
        this.reconciling.set(false);
        this.editingDetails.set(false);
        this.detail.set(d);
        this.feedback.notify('Updated the product details.');
      },
      error: (e: unknown) => {
        this.reconciling.set(false);
        this.feedback.error(`Could not update the product${onlineHint(e)}`);
      },
    });
  }

  // --- Reconciliation: approve where the sources disagree with the product ---

  /** The "keep the current value" choice — the backend's `Choice::Keep`. */
  static readonly KEEP: Choice = 'keep';

  /** Fields where a source disagrees with the canonical product and you haven't
   *  decided yet. Empty (so the section is hidden) when everything agrees. */
  readonly reconFields = computed(() => this.detail()?.reconciliation.fields ?? []);

  /** Your per-field pick, keyed by field. Absent → "keep" (the safe default:
   *  nothing changes unless you choose a source). */
  readonly choices = signal<Partial<Record<ReconcileField, Choice>>>({});
  readonly reconciling = signal(false);

  choiceFor(field: ReconcileField): Choice {
    return this.choices()[field] ?? ProductPage.KEEP;
  }

  setChoice(field: ReconcileField, choice: Choice): void {
    this.choices.update((c) => ({ ...c, [field]: choice }));
  }

  /** A source id → its display name, for the candidate labels. */
  label(source: Source): string {
    return sourceLabel(source);
  }

  /** Settle every shown difference at once: each field is either adopted from a
   *  source or kept as-is (the default). Sending a decision for all of them —
   *  including the kept ones — is what marks the review done, so it won't nag
   *  again until a source's value actually changes. */
  applyReconcile(): void {
    const fields = this.reconFields();
    if (!fields.length || this.reconciling()) return;
    const decisions: FieldChoice[] = fields.map((f) => ({
      field: f.field,
      choice: this.choiceFor(f.field),
    }));
    this.reconciling.set(true);
    this.api.reconcile(this.id(), decisions).subscribe({
      next: (d) => {
        this.reconciling.set(false);
        this.choices.set({});
        this.detail.set(d);
        this.feedback.notify('Updated the product details.');
      },
      error: (e: unknown) => {
        this.reconciling.set(false);
        this.feedback.error(`Could not update the product${onlineHint(e)}`);
      },
    });
  }

  // --- Asda's full details (nutrition/ingredients/allergens from its page) ---
  //
  // The Asda SEARCH API carries no facts; they live on the product page, behind
  // Cloudflare. The hidden WebView (Android app only) fetches the raw blob; the
  // server parses it. Offered only when the bridge is present AND we already have
  // an Asda listing whose barcode this product was confirmed against.

  readonly fetchingFacts = signal(false);

  /** The product's Asda listing, if any — its CIN is the page to fetch. */
  private readonly asdaListing = computed(() =>
    this.detail()?.listings.find((l) => l.source === 'asda'),
  );

  /** Only inside the app, and only once an Asda listing exists to enrich. */
  readonly canGetAsdaFacts = computed(() => this.shops.available && !!this.asdaListing());

  /** The Asda page blob we've already fetched and stored, if any — so the action
   *  reads as a refresh (with when) rather than a first fetch, and viewing the
   *  product never re-fetches what we hold. */
  readonly asdaFactsDoc = computed(() =>
    this.detail()?.documents.find((d) => d.source === 'asda' && d.kind === 'page'),
  );

  /** "stored today / 3 days ago" for the held Asda page blob. */
  readonly asdaFactsAge = computed(() => {
    const doc = this.asdaFactsDoc();
    return doc ? ago(doc.fetched_at) : null;
  });

  /** Pull Asda's product-page facts through the WebView and store them. The blob
   *  goes to the server untouched; the server parses and barcode-gates it. */
  getAsdaFacts(): void {
    const listing = this.asdaListing();
    if (!listing || this.fetchingFacts()) return;
    this.fetchingFacts.set(true);
    this.shops
      .fetchFacts(ASDA_FACTS, listing.external_id)
      .then((f) =>
        this.api.submitFacts(this.id(), { source: 'asda', ean: f.ean, blob: f.blob }).subscribe({
          next: (d) => {
            this.fetchingFacts.set(false);
            this.detail.set(d);
            this.feedback.notify('Added Asda’s full details.');
          },
          error: (e: unknown) => {
            this.fetchingFacts.set(false);
            this.feedback.error(`Could not save Asda’s details${onlineHint(e)}`);
          },
        }),
      )
      .catch((e: unknown) => {
        this.fetchingFacts.set(false);
        this.feedback.error(
          `Could not read Asda’s page: ${e instanceof Error ? e.message : String(e)}`,
        );
      });
  }

  readonly imageUrl = computed(() => {
    const d = this.detail();
    return d?.product.has_image ? this.api.productImageByIdUrl(d.product.id) : null;
  });

  /** "Brand · 500g" — whichever parts exist. */
  readonly subtitle = computed(() => {
    const p = this.detail()?.product;
    return [p?.brand, p?.quantity_label].filter((s) => !!s).join(' · ');
  });

  /** Shops, cheapest first, then any shop with no price yet that still has a
   *  page to link to. The 'off' listing is attribution, not a shop (below).
   *
   *  A price names the exact listing it came from, so the link goes to the item
   *  actually quoted — a shop with two listings for one product is already
   *  collapsed to its cheapest by the backend. */
  /**
   * What this person has paid, newest first — deliberately NOT merged into
   * `buyRows`.
   *
   * A shelf price is what a shop charges today; a purchase is what one person
   * paid on one day. Putting them in one list would make each read as the other:
   * a two-year-old receipt would look like a current quote, and today's quote
   * would look like proof you can get it at that price. They answer different
   * questions, so they get different lists.
   */
  readonly paidRows = computed(() =>
    (this.detail()?.purchases ?? []).map((p) => ({
      id: p.id,
      shop: p.shop,
      price: `${p.currency === 'GBP' ? '£' : p.currency + ' '}${fromMinorUnits(p.amount_minor)}`,
      // The pack it was for. Without it £3.30 cannot be compared with £3.30.
      pack: p.quantity != null ? `${p.quantity}${p.unit ? ' ' + p.unit : ''}` : '',
      when: new Date(p.bought_at).toLocaleDateString(),
    })),
  );

  readonly buyRows = computed<BuyRow[]>(() => {
    const d = this.detail();
    if (!d) return [];
    const shops = d.listings.filter((l) => l.source !== 'off');
    const listing = new Map<string, ProductListing>(shops.map((l) => [listingKey(l), l]));
    const rows: BuyRow[] = d.prices.map((p) => {
      const key = listingKey(p);
      return {
        key,
        label: sourceLabel(p.source),
        externalId: p.external_id,
        source: p.source,
        url: listing.get(key)?.url ?? null,
        price: money(p.amount_minor, p.currency),
        perUnit:
          p.unit_amount_minor != null && p.unit_measure
            ? `${money(p.unit_amount_minor, p.currency)}/${p.unit_measure}`
            : null,
        observed: ago(p.observed_at),
      };
    });
    const priced = new Set(d.prices.map((p) => p.source));
    for (const l of shops) {
      // One link per unpriced shop, not per listing: two Asda listings with no
      // price are still one "Asda" line, mirroring the priced side.
      if (!priced.has(l.source) && l.url && !rows.some((r) => r.label === sourceLabel(l.source))) {
        rows.push({
          key: listingKey(l),
          label: sourceLabel(l.source),
          externalId: l.external_id,
          source: l.source,
          url: l.url,
          price: null,
          perUnit: null,
          observed: null,
        });
      }
    }
    return rows;
  });

  /** The Open Food Facts page, for the data-attribution line. */
  readonly offUrl = computed(
    () => this.detail()?.listings.find((l) => l.source === 'off')?.url ?? null,
  );

  /** The UK panel, in its statutory order, "of which" rows indented; rows the
   *  source didn't declare are omitted rather than shown as blanks. */
  readonly nutrientRows = computed<NutrientRow[]>(() => {
    const n = this.detail()?.facts.nutrition;
    if (!n) return [];
    const rows: NutrientRow[] = [];
    const energy = [
      n.energy_kj != null ? `${n.energy_kj} kJ` : null,
      n.energy_kcal != null ? `${n.energy_kcal} kcal` : null,
    ]
      .filter((s) => s !== null)
      .join(' / ');
    if (energy) rows.push({ label: 'Energy', value: energy, sub: false });
    const grams: [string, number | null, boolean][] = [
      ['Fat', n.fat_g, false],
      ['of which saturates', n.saturates_g, true],
      ['Carbohydrate', n.carbohydrate_g, false],
      ['of which sugars', n.sugars_g, true],
      ['Fibre', n.fibre_g, false],
      ['Protein', n.protein_g, false],
      ['Salt', n.salt_g, false],
    ];
    for (const [label, v, sub] of grams) {
      if (v != null) rows.push({ label, value: `${v} g`, sub });
    }
    return rows;
  });

  readonly basis = computed(() => this.detail()?.facts.nutrition?.basis ?? '100g');
  readonly servingSize = computed(() => this.detail()?.facts.nutrition?.serving_size ?? null);

  readonly contains = computed(
    () =>
      this.detail()
        ?.facts.allergens.filter((a) => a.presence === 'contains')
        .map((a) => humanize(a.allergen)) ?? [],
  );
  readonly mayContain = computed(
    () =>
      this.detail()
        ?.facts.allergens.filter((a) => a.presence === 'may_contain')
        .map((a) => humanize(a.allergen)) ?? [],
  );

  readonly dietary = computed<DietaryChip[]>(
    () =>
      this.detail()?.facts.dietary.map((f) => ({
        label: humanize(f.flag),
        value: f.value,
      })) ?? [],
  );

  /** The safety-critical facts (allergens, dietary) where the sources disagree —
   *  surfaced as provenance. These never reconcile to a single-source pick (an
   *  allergen any source flags is kept; a disputed diet claim shows "maybe"), so
   *  the honest thing is to show who said what and send you to the label. Empty
   *  unless there are two+ sources that actually differ. */
  readonly factProvenance = computed<FactConflict[]>(() => {
    const bySrc = this.detail()?.facts_by_source ?? [];
    if (bySrc.length < 2) return [];
    const out: FactConflict[] = [];

    // Dietary flags asserted with different values across sources.
    const flags = new Set<string>();
    for (const s of bySrc) for (const f of s.facts.dietary) flags.add(f.flag);
    for (const flag of [...flags].sort()) {
      const per = bySrc
        .map((s) => ({
          source: s.source,
          value: s.facts.dietary.find((f) => f.flag === flag)?.value,
        }))
        .filter((x): x is { source: Source; value: Claim } => !!x.value);
      if (new Set(per.map((x) => x.value)).size > 1) {
        out.push({
          label: humanize(flag),
          perSource: per.map((x) => ({ source: this.label(x.source), value: x.value })),
        });
      }
    }

    // Allergens where the sources disagree — including one being silent, which
    // is safety-relevant (silence is not a "free from").
    const names = new Set<string>();
    for (const s of bySrc) for (const a of s.facts.allergens) names.add(a.allergen);
    for (const name of [...names].sort()) {
      const per = bySrc.map((s) => {
        const a = s.facts.allergens.find((x) => x.allergen === name);
        const value = a ? (a.presence === 'contains' ? 'contains' : 'may contain') : 'not listed';
        return { source: this.label(s.source), value };
      });
      if (new Set(per.map((x) => x.value)).size > 1) {
        out.push({ label: `Allergen: ${humanize(name)}`, perSource: per });
      }
    }
    return out;
  });
}
