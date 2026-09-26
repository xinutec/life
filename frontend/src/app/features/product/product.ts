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
  Source,
} from '../../models';
import { ago } from '../../shared/ago';
import { assertNever, classifyApiError, onlineHint } from '../../shared/api-error';
import { ProductImages } from '../../product-image';
import { ProductShops } from './product-shops';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { formatMoney } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { Shops } from '../../shop';
import { ASDA_FACTS } from '../../shops/asda';

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
  private images = inject(ProductImages);
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

  /** Finding this product at shops, and refreshing the listed ones. */
  readonly finder = new ProductShops(
    () => this.id(),
    this.detail,
    () => this.reload(),
  );

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
  // A shop's own casing ("250ML") that no source disagrees with can only be
  // fixed in our own layer.

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

  /** A picture is being uploaded — the button is disabled and says so, because
   *  a photo takes long enough on a phone that a silent wait reads as broken. */
  readonly savingImage = signal(false);

  /**
   * Give this product a picture from a file the person picks.
   *
   * Keyed on the barcode, which is what the endpoint takes; a shop product
   * with no EAN gets no control rather than a failing one. No `capture` on the
   * input: it would force the camera and hide the photo library.
   */
  pickImage(ev: Event): void {
    // A guard, not a cast: `ev.target` is EventTarget and narrowing it by
    // assertion is a claim the compiler cannot check (the lint says so).
    const input = ev.target;
    if (!(input instanceof HTMLInputElement)) return;
    const file = input.files?.[0];
    // Clear first: picking the SAME file twice fires no change event otherwise,
    // so a failed upload could not be retried with the same photo.
    input.value = '';
    if (!file) return;
    const barcode = this.detail()?.product.barcode;
    if (!barcode) return;
    this.savingImage.set(true);
    this.images.replace(barcode, file).subscribe({
      next: () => {
        this.savingImage.set(false);
        // Re-read so `has_image` flips on for a product that had none — without
        // it the <img> stays hidden and the upload reads as having failed.
        this.reload();
        this.feedback.notify('Picture saved.');
      },
      error: (e: unknown) => {
        this.savingImage.set(false);
        this.feedback.error(`Could not save the picture${onlineHint(e)}`);
      },
    });
  }

  /** "Brand · 500g" — whichever parts exist. */
  readonly subtitle = computed(() => {
    const p = this.detail()?.product;
    return [p?.brand, p?.quantity_label].filter((s) => !!s).join(' · ');
  });

  /** What this person has paid, newest first; deliberately a separate list
   *  from `buyRows`, so an old receipt never reads as a current quote. */
  readonly paidRows = computed(() =>
    (this.detail()?.purchases ?? []).map((p) => ({
      id: p.id,
      shop: p.shop,
      price: formatMoney(p.amount_minor, p.currency),
      // The RATE first, because that is the comparable number and the whole
      // reason the pack is captured; the pack itself only says what the rate is
      // of. Absent when the unit could not be read — see purchases::repo.
      pack: [
        p.unit_amount_minor != null && p.unit_measure
          ? `${formatMoney(p.unit_amount_minor, p.currency)}/${p.unit_measure}`
          : '',
        p.quantity != null ? `${p.quantity}${p.unit ? ' ' + p.unit : ''}` : '',
      ]
        .filter((x) => x)
        .join(' · '),
      when: ago(new Date(p.bought_at).getTime()),
    })),
  );

  /** Present parts joined by a non-breaking " · ". */
  protected dotted(...parts: (string | null | undefined)[]): string {
    return parts.filter(Boolean).join('\u00a0·\u00a0');
  }

  /** Shops, cheapest first, then unpriced shops that still have a page to link.
   *  A price links to the listing it came from; the backend has already
   *  collapsed a shop's listings to its cheapest. 'off' is attribution, not a
   *  shop. */
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
        price: formatMoney(p.amount_minor, p.currency),
        perUnit:
          p.unit_amount_minor != null && p.unit_measure
            ? `${formatMoney(p.unit_amount_minor, p.currency)}/${p.unit_measure}`
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
