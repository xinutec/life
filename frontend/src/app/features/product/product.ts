import { Location } from '@angular/common';
import { Component, computed, effect, inject, input, numberAttribute, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

import { LifeApi } from '../../life-api';
import { AsdaHit, ProductDetail, ProductListing } from '../../models';
import { assertNever, classifyApiError, onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { sourceLabel } from '../../shared/sources';

/** One "where to buy" line: a shop that lists the product, with its current
 *  price (when one has been observed) and a deep link to its product page.
 *  `key` is the listing's identity — a shop can appear once, but the label is
 *  a display string and must never be used to identify a row. */
interface BuyRow {
  key: string;
  label: string;
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

/** A dietary chip: the flag humanized, styled by its tri-state value. */
interface DietaryChip {
  label: string;
  value: 'yes' | 'no' | 'maybe';
}

/** How the shop lookup is going. `none` = searched, nothing carried this
 *  barcode — a real answer, not an error. */
type ShopLookup = 'idle' | 'searching' | 'found' | 'none' | 'error';

/** The one hit that IS this product, or null.
 *
 *  Identity is the barcode, never the name: neither Asda nor Waitrose supports
 *  barcode→product lookup, so we can only reach a shop by NAME search — and a
 *  name search for "Asda ES Balsamic Modena" ranks a *raspberry* glaze above
 *  the product itself. Every hit carries its EAN, so we ignore the shop's
 *  relevance order entirely and take the one whose barcode matches. A hit that
 *  merely reads alike is a DIFFERENT product and must never be attached — the
 *  same precision-over-recall rule the visit matcher follows.
 *
 *  Pure, so the rule is tested without a network. */
export function eanMatch(hits: AsdaHit[], barcode: string): AsdaHit | null {
  return hits.find((h) => h.barcode != null && h.barcode === barcode) ?? null;
}

/** A listing's identity — what joins a price to the listing that quoted it, and
 *  what keys a row. `(source, external_id)` is the listing's unique key. */
function listingKey(l: { source: string; external_id: string }): string {
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

/** Epoch millis → "today" / "yesterday" / "n days ago" / a date. Price freshness
 *  is the point: a shelf price observed weeks ago should read as stale. */
function ago(epochMs: number): string {
  const days = Math.floor((Date.now() - epochMs) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(epochMs).toLocaleDateString();
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
  imports: [MatButtonModule, MatIconModule, ListState],
})
export class ProductPage {
  /** The routed product id. Route params arrive as strings (see
   *  withComponentInputBinding); `numberAttribute` is the one place that
   *  conversion happens — a junk id becomes NaN and is caught in `load`. */
  readonly id = input.required({ transform: numberAttribute });

  private api = inject(LifeApi);
  private location = inject(Location);
  private feedback = inject(Feedback);

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
  // Asda only, deliberately: its storefront search is a public, CORS-open API we
  // can call from anywhere (see products::asda), so this works in the browser.
  // Waitrose needs the Android app's hidden WebView to get past its bot-wall, so
  // it stays in the picker's shop tier rather than being half-offered here.

  readonly shopLookup = signal<ShopLookup>('idle');
  readonly shopMatch = signal<AsdaHit | null>(null);
  readonly attaching = signal(false);

  /** Only offer the lookup when it can give a truthful answer: we need a barcode
   *  to match on, and there's nothing to find if Asda already lists it. */
  readonly canFindAtAsda = computed(() => {
    const d = this.detail();
    if (!d?.product.barcode) return false;
    return !d.listings.some((l) => l.source === 'asda');
  });

  /** Search Asda by name, then keep only a barcode-confirmed hit. */
  findAtAsda(): void {
    const d = this.detail();
    const barcode = d?.product.barcode;
    const query = d?.product.name?.trim();
    if (!barcode || !query) return;
    this.shopLookup.set('searching');
    this.shopMatch.set(null);
    this.api.searchAsda(query).subscribe({
      next: (hits) => {
        const match = eanMatch(hits, barcode);
        this.shopMatch.set(match);
        this.shopLookup.set(match ? 'found' : 'none');
      },
      error: () => this.shopLookup.set('error'),
    });
  }

  /** Attach the confirmed hit: importing under ITS OWN barcode (equal to ours,
   *  which is why it matched) lets the backend reconcile the two onto one
   *  product — we never force our barcode onto a shop's listing. */
  attach(hit: AsdaHit): void {
    if (this.attaching()) return;
    this.attaching.set(true);
    this.api
      .importProduct({
        source: 'asda',
        external_id: hit.external_id,
        name: hit.name,
        brand: hit.brand,
        barcode: hit.barcode,
        image_url: hit.image_url,
        price: hit.price,
      })
      .subscribe({
        next: () => {
          this.attaching.set(false);
          this.shopLookup.set('idle');
          this.shopMatch.set(null);
          this.feedback.notify('Added Asda.');
          this.reload();
        },
        error: (e: unknown) => {
          this.attaching.set(false);
          this.feedback.error(`Could not add Asda${onlineHint(e)}`);
        },
      });
  }

  back(): void {
    this.location.back();
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
        value: f.value as DietaryChip['value'],
      })) ?? [],
  );
}
