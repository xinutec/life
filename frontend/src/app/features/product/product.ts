import { Location } from '@angular/common';
import { Component, computed, effect, inject, input, numberAttribute, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatRadioModule } from '@angular/material/radio';

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
  /** The source's own id for the listing — what a refresh re-reads. */
  externalId: string;
  source: string;
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
  /** Whether the match came from what we already knew rather than a fresh
   *  query. Shown, not hidden: a cache you can't see is a cache you can't
   *  catch being wrong. */
  readonly fromCache = signal(false);
  readonly attaching = signal(false);

  /** Only offer the lookup when it can give a truthful answer: we need a barcode
   *  to match on, and there's nothing to find if Asda already lists it. */
  readonly canFindAtAsda = computed(() => {
    const d = this.detail();
    if (!d?.product.barcode) return false;
    return !d.listings.some((l) => l.source === 'asda');
  });

  /** Ask whether Asda carries this barcode. The backend checks what past shop
   *  queries already taught it before searching, so this often costs Asda
   *  nothing; either way it matches on the EAN and hands back only a confirmed
   *  hit. */
  findAtAsda(): void {
    if (!this.canFindAtAsda()) return;
    this.shopLookup.set('searching');
    this.shopMatch.set(null);
    this.fromCache.set(false);
    this.api.findAtShop(this.id(), 'asda').subscribe({
      next: (found) => {
        this.shopMatch.set(found.hit);
        this.fromCache.set(found.from_cache);
        this.shopLookup.set(found.hit ? 'found' : 'none');
      },
      error: () => this.shopLookup.set('error'),
    });
  }

  /** "Brand · 400G" for a shop hit we haven't imported yet — the same subtitle
   *  shape as the product page, from whatever the shop handed back. Lets you
   *  size up the match before committing to Add. */
  hitSubtitle(hit: AsdaHit): string {
    return [hit.brand, hit.quantity_label].filter((s) => !!s).join(' · ');
  }

  /** Attach the barcode-confirmed hit. The backend re-fetches it shop-side and
   *  re-checks the barcode itself, so the match this screen made is a
   *  convenience, not something the server takes on trust. */
  attach(hit: AsdaHit): void {
    this.pull(hit.external_id, 'Added Asda.', 'Could not add Asda');
  }

  /** Re-read a shop listing on demand — pressed when you've seen the shelf price
   *  change. Nothing refetches on a timer: shop data goes stale silently, and a
   *  wrong price you didn't ask for is worse than an old one you can refresh. */
  refresh(row: BuyRow): void {
    this.pull(row.externalId, `Refreshed ${row.label}.`, `Could not refresh ${row.label}`);
  }

  private pull(externalId: string, ok: string, bad: string): void {
    if (this.attaching()) return;
    this.attaching.set(true);
    this.api.syncListing(this.id(), 'asda', externalId).subscribe({
      next: () => {
        this.attaching.set(false);
        this.shopLookup.set('idle');
        this.shopMatch.set(null);
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

  // --- Reconciliation: approve where the sources disagree with the product ---

  /** The "keep the current value" choice — mirrors the backend's KEEP. */
  static readonly KEEP = 'keep';

  /** Fields where a source disagrees with the canonical product and you haven't
   *  decided yet. Empty (so the section is hidden) when everything agrees. */
  readonly reconFields = computed(() => this.detail()?.reconciliation.fields ?? []);

  /** Your per-field pick, keyed by field. Absent → "keep" (the safe default:
   *  nothing changes unless you choose a source). */
  readonly choices = signal<Record<string, string>>({});
  readonly reconciling = signal(false);

  choiceFor(field: string): string {
    return this.choices()[field] ?? ProductPage.KEEP;
  }

  setChoice(field: string, choice: string): void {
    this.choices.update((c) => ({ ...c, [field]: choice }));
  }

  /** A source id → its display name, for the candidate labels. */
  label(source: string): string {
    return sourceLabel(source);
  }

  /** Settle every shown difference at once: each field is either adopted from a
   *  source or kept as-is (the default). Sending a decision for all of them —
   *  including the kept ones — is what marks the review done, so it won't nag
   *  again until a source's value actually changes. */
  applyReconcile(): void {
    const fields = this.reconFields();
    if (!fields.length || this.reconciling()) return;
    const decisions = fields.map((f) => ({ field: f.field, choice: this.choiceFor(f.field) }));
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
        value: f.value as DietaryChip['value'],
      })) ?? [],
  );
}
