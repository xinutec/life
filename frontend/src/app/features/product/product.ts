import { Component, computed, effect, inject, input, numberAttribute, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatRadioModule } from '@angular/material/radio';
import { scaffoldTitle } from '@xinutec/ui-scaffold';

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
import { amount } from '../../shared/amount';
import { assertNever, classifyApiError, onlineHint } from '../../shared/api-error';
import { MAX_IMAGE_BYTES } from '../../image-picker';
import { ProductImages } from '../../product-image';
import { ProductShops } from './product-shops';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { formatMoney, formatUnitPrice } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { Shops } from '../../shop';
import { ASDA_FACTS } from '../../shops/asda';

/** A shop that lists the product, with its price if seen and a link. */
interface BuyRow {
  key: string;
  label: string;
  externalId: string;
  source: Source;
  url: string | null;
  price: string | null;
  perUnit: string | null;
  observed: string | null;
}

/** `sub` marks the "of which" rows. */
interface NutrientRow {
  label: string;
  value: string;
  sub: boolean;
}

interface DietaryChip {
  label: string;
  value: Claim;
}

/** A safety fact the sources disagree on, shown source by source. */
interface FactConflict {
  label: string;
  perSource: { source: string; value: string }[];
}

/** `(source, external_id)`: what joins a price to its listing. */
function listingKey(l: { source: Source; external_id: string }): string {
  return `${l.source}/${l.external_id}`;
}

/** "gluten_free" → "Gluten free". */
function humanize(slug: string): string {
  const words = slug.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Everything known about a product, on one screen. */
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
  /** A junk id becomes NaN and is caught in `load`. */
  readonly id = input.required({ transform: numberAttribute });

  private api = inject(LifeApi);
  private feedback = inject(Feedback);
  private images = inject(ProductImages);
  private shops = inject(Shops);

  readonly detail = signal<ProductDetail | null>(null);
  readonly loading = signal(true);
  readonly error = signal(false);
  readonly errorText = signal('');

  constructor() {
    scaffoldTitle(() => 'Product');
    effect(() => this.load(this.id()));
  }

  private load(id: number): void {
    this.detail.set(null);
    if (!Number.isFinite(id)) {
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

  readonly finder = new ProductShops(
    () => this.id(),
    this.detail,
    () => this.reload(),
  );

  readonly editingName = signal(false);
  readonly nameDraft = signal('');

  startEditName(): void {
    this.nameDraft.set(this.detail()?.product.name ?? '');
    this.editingName.set(true);
  }

  cancelEditName(): void {
    this.editingName.set(false);
  }

  /** Our own name outranks every source; through reconcile, so it also settles
   *  the name divergence. */
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

  // Our own brand and pack: the only way to fix a casing no source disputes.

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

  /** Sends only fields changed to a non-empty value: this corrects, it does
   *  not clear. */
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

  static readonly KEEP: Choice = 'keep';

  readonly reconFields = computed(() => this.detail()?.reconciliation.fields ?? []);

  /** Absent means keep. */
  readonly choices = signal<Partial<Record<ReconcileField, Choice>>>({});
  readonly reconciling = signal(false);

  choiceFor(field: ReconcileField): Choice {
    return this.choices()[field] ?? ProductPage.KEEP;
  }

  setChoice(field: ReconcileField, choice: Choice): void {
    this.choices.update((c) => ({ ...c, [field]: choice }));
  }

  label(source: Source): string {
    return sourceLabel(source);
  }

  /** A decision for every field, kept ones too: that marks the review done. */
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
        // An adopted picture keeps its URL, so the <img> must be told.
        if (decisions.some((c) => c.field === 'picture' && c.choice !== ProductPage.KEEP)) {
          this.images.changed(d.product);
        }
        this.detail.set(d);
        this.feedback.notify('Updated the product details.');
      },
      error: (e: unknown) => {
        this.reconciling.set(false);
        this.feedback.error(`Could not update the product${onlineHint(e)}`);
      },
    });
  }

  // Asda's facts live on its bot-walled page: the app's WebView fetches the
  // blob and the server parses it.

  readonly fetchingFacts = signal(false);

  private readonly asdaListing = computed(() =>
    this.detail()?.listings.find((l) => l.source === 'asda'),
  );

  readonly canGetAsdaFacts = computed(() => this.shops.available && !!this.asdaListing());

  /** The stored Asda page, so the action reads as a refresh. */
  readonly asdaFactsDoc = computed(() =>
    this.detail()?.documents.find((d) => d.source === 'asda' && d.kind === 'page'),
  );

  readonly asdaFactsAge = computed(() => {
    const doc = this.asdaFactsDoc();
    return doc ? ago(doc.fetched_at) : null;
  });

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
    return d?.product.has_image ? this.images.urlById(d.product.id) : null;
  });

  readonly savingImage = signal(false);

  /** Keyed on the barcode, as the endpoint is: a product without one gets no
   *  control. No `capture`, which would hide the photo library. */
  pickImage(ev: Event): void {
    const input = ev.target;
    if (!(input instanceof HTMLInputElement)) return;
    const file = input.files?.[0];
    // Cleared, so the same file can be picked again.
    input.value = '';
    if (!file) return;
    const barcode = this.detail()?.product.barcode;
    if (!barcode) return;
    if (!file.type.startsWith('image/')) {
      this.feedback.error('That’s not an image.');
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      this.feedback.error('Image is larger than 5 MB.');
      return;
    }
    this.savingImage.set(true);
    this.images.replace(barcode, file, this.id()).subscribe({
      next: () => {
        this.savingImage.set(false);
        // Re-read so `has_image` turns on.
        this.reload();
        this.feedback.notify('Picture saved.');
      },
      error: (e: unknown) => {
        this.savingImage.set(false);
        this.feedback.error(`Could not save the picture${onlineHint(e)}`);
      },
    });
  }

  readonly subtitle = computed(() => {
    const p = this.detail()?.product;
    return [p?.brand, p?.quantity_label].filter((s) => !!s).join(' · ');
  });

  /** What was paid, apart from `buyRows`: an old receipt is not a quote. */
  readonly paidRows = computed(() =>
    (this.detail()?.purchases ?? []).map((p) => ({
      id: p.id,
      shop: p.shop,
      price: formatMoney(p.amount_minor, p.currency),
      // The rate first: it is the comparable number.
      pack: [
        p.unit_price ? formatUnitPrice(p.unit_price, p.currency) : '',
        amount(p.quantity, p.unit),
      ]
        .filter((x) => x)
        .join(' · '),
      when: ago(p.bought_at),
    })),
  );

  protected dotted(...parts: (string | null | undefined)[]): string {
    return parts.filter(Boolean).join('\u00a0·\u00a0');
  }

  /** Shops cheapest first, then unpriced shops with a page. `off` is
   *  attribution, not a shop. */
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
        perUnit: p.unit_price ? formatUnitPrice(p.unit_price, p.currency) : null,
        observed: ago(p.observed_at),
      };
    });
    const priced = new Set(d.prices.map((p) => p.source));
    for (const l of shops) {
      // One line per unpriced shop, as on the priced side.
      if (!priced.has(l.source) && l.url && !rows.some((r) => r.source === l.source)) {
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

  readonly offUrl = computed(
    () => this.detail()?.listings.find((l) => l.source === 'off')?.url ?? null,
  );

  /** In statutory order; undeclared rows are left out. */
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

  /** Allergens and diets the sources disagree on. These never reconcile to one
   *  source, so who said what is shown instead. */
  readonly factProvenance = computed<FactConflict[]>(() => {
    const bySrc = this.detail()?.facts_by_source ?? [];
    if (bySrc.length < 2) return [];
    const out: FactConflict[] = [];

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

    // A silent source counts as disagreeing: silence is not "free from".
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
