import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import {
  Subject,
  catchError,
  debounceTime,
  distinctUntilChanged,
  firstValueFrom,
  of,
  switchMap,
} from 'rxjs';

import { LifeApi } from '../life-api';
import { AsdaHit, Item, ItemCategory, Product } from '../models';
import { ShopCandidate, ShopProvider, Shops } from '../shop';
import { WAITROSE } from '../shops/waitrose';
import { ItemsStore } from '../stores/catalog';
import { Dialog } from './dialog';
import { Feedback } from './feedback';

export interface ProductPickData {
  /** Prefill (usually what's typed in the Name field); searched immediately. */
  initialQuery: string;
}

/** What a pick hands back: enough to fill an add/edit form. `unit`/`category`
 *  ride along only from an inventory hit (a catalog product knows neither). */
export interface ProductPick {
  name: string;
  barcode: string | null;
  product_id: number | null;
  unit: string | null;
  category: ItemCategory | null;
}

/** Inventory items whose name or brand contains the query (case-insensitive):
 *  the instant, offline tier — things bought before are inventory items, so
 *  this IS the purchase history. Name-prefix hits sort first. */
export function localHits(items: Item[], query: string): Item[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const prefix = (it: Item) => (it.name.toLowerCase().startsWith(q) ? 0 : 1);
  return items
    .filter(
      (it) =>
        it.name.toLowerCase().includes(q) || (it.brand ?? '').toLowerCase().includes(q),
    )
    .sort((a, b) => prefix(a) - prefix(b) || a.name.localeCompare(b.name))
    .slice(0, 8);
}

/** Catalog rows not already represented by a local hit (same catalog id or
 *  barcode) — a product shouldn't appear twice under two section headers. */
export function withoutLocalDupes(catalog: Product[], locals: Item[]): Product[] {
  const ids = new Set(locals.map((it) => it.product_id).filter((v) => v != null));
  const codes = new Set(locals.map((it) => it.barcode).filter((v) => v != null));
  return catalog.filter(
    (p) => !ids.has(p.id) && (p.barcode == null || !codes.has(p.barcode)),
  );
}

/** A search that finds nothing usually means "not signed in" (the shop only
 *  returns results to a logged-in session), so nudge toward Connect. */
function shopMessage(e: unknown, provider: ShopProvider): string {
  const raw = e instanceof Error ? e.message : String(e);
  return /only available in the app/i.test(raw)
    ? raw
    : `Search failed — try “Connect ${provider.displayName}” first.`;
}

/** Shops offered in the shop tier. Adding one (Asda) is a single entry here plus
 *  its `shops/<shop>.ts` provider. */
const PROVIDERS: ShopProvider[] = [WAITROSE];

/** Find-a-product dialog, shared by the Buy and Inventory sheets. One query,
 *  three tiers by cost: inventory items (instant, offline), the product
 *  catalog (one cheap API call, debounced), and — inside the Android app — an
 *  explicit shop search (a hidden WebView on the shop site; picking a shop hit
 *  imports it into the catalog first). Closes with a [[ProductPick]], or null
 *  if cancelled. */
@Component({
  selector: 'app-product-picker',
  templateUrl: './product-picker.html',
  styleUrl: './product-picker.scss',
  imports: [
    Dialog,
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatListModule,
    MatProgressSpinnerModule,
  ],
})
export class ProductPicker {
  private ref = inject<MatDialogRef<ProductPicker, ProductPick | null>>(MatDialogRef);
  private data = inject<ProductPickData>(MAT_DIALOG_DATA);
  private api = inject(LifeApi);
  private shops = inject(Shops);
  private itemsStore = inject(ItemsStore);
  private feedback = inject(Feedback);

  /** Shop tier only works inside the Android app (needs the native bridge). */
  readonly shopProviders = this.shops.available ? PROVIDERS : [];

  readonly query = signal(this.data.initialQuery.trim());
  private readonly query$ = new Subject<string>();

  /** Catalog tier: debounced server search. A failure is just an empty tier —
   *  the local tier still answers offline. */
  private readonly catalogRaw = toSignal(
    this.query$.pipe(
      debounceTime(250),
      distinctUntilChanged(),
      switchMap((q) =>
        q ? this.api.searchProducts(q).pipe(catchError(() => of([] as Product[]))) : of([] as Product[]),
      ),
    ),
    { initialValue: [] as Product[] },
  );

  /** Asda tier: same debounced query, but a live search against Asda's
   *  storefront (backend → Algolia). Works everywhere (no app bridge). A failed
   *  search is just an empty tier. */
  readonly asda = toSignal(
    this.query$.pipe(
      debounceTime(250),
      distinctUntilChanged(),
      switchMap((q) =>
        q ? this.api.searchAsda(q).pipe(catchError(() => of([] as AsdaHit[]))) : of([] as AsdaHit[]),
      ),
    ),
    { initialValue: [] as AsdaHit[] },
  );

  readonly locals = computed(() => localHits(this.itemsStore.value() ?? [], this.query()));
  readonly catalog = computed(() => withoutLocalDupes(this.catalogRaw(), this.locals()));

  readonly shopResults = signal<ShopCandidate[] | null>(null);
  readonly shopBusy = signal(false);
  readonly shopError = signal<string | null>(null);
  readonly importing = signal(false);

  constructor() {
    this.itemsStore.refresh();
    if (this.query()) this.query$.next(this.query());
  }

  queryChanged(value: string): void {
    this.query.set(value);
    this.query$.next(value.trim());
    // Shop hits answered an older query — stale results mislead; re-search.
    this.shopResults.set(null);
    this.shopError.set(null);
  }

  /** The cached image for a row, or null → placeholder icon. Plain <img>, not
   *  [[ProductThumb]]: the thumb is itself a tap-to-replace picker, which would
   *  fight the row's own pick tap. */
  thumbUrl(barcode: string | null, productId: number | null, hasImage: boolean): string | null {
    if (!hasImage) return null;
    if (barcode != null) return this.api.productImageUrl(barcode);
    if (productId != null) return this.api.productImageByIdUrl(productId);
    return null;
  }

  sourceLabel(p: Product): string {
    switch (p.source) {
      case 'off':
        return 'Open Food Facts';
      case 'user':
        return 'added by you';
      case null:
        return '';
      default:
        return p.source; // a shop id ('waitrose', …) reads fine as-is
    }
  }

  pickItem(it: Item): void {
    this.ref.close({
      name: it.name,
      barcode: it.barcode,
      product_id: it.product_id,
      unit: it.unit,
      category: it.category,
    });
  }

  pickProduct(p: Product): void {
    this.ref.close({
      name: p.name ?? this.query(),
      barcode: p.barcode,
      product_id: p.id,
      unit: null,
      category: null,
    });
  }

  /** Import the Asda product into the catalog (server caches its scene7 image),
   *  then close linked. The imported catalog row is barcodeless (keyed by CIN),
   *  but the hit's EAN rides along so the shopping/inventory row still carries a
   *  barcode. */
  pickAsda(hit: AsdaHit): void {
    if (this.importing()) return;
    this.importing.set(true);
    firstValueFrom(
      this.api.importProduct({
        source: 'asda',
        external_id: hit.external_id,
        name: hit.name,
        brand: hit.brand,
        image_url: hit.image_url,
      }),
    )
      .then((product) =>
        this.ref.close({
          name: product.name ?? hit.name,
          barcode: hit.barcode ?? product.barcode,
          product_id: product.id,
          unit: null,
          category: null,
        }),
      )
      .catch(() => this.feedback.error('Could not link the Asda product.'))
      .finally(() => this.importing.set(false));
  }

  searchShop(provider: ShopProvider): void {
    const q = this.query().trim();
    if (!q || this.shopBusy()) return;
    this.shopBusy.set(true);
    this.shopError.set(null);
    this.shops
      .search(provider, q)
      .then((candidates) => this.shopResults.set(candidates))
      .catch((e: unknown) => this.shopError.set(shopMessage(e, provider)))
      .finally(() => this.shopBusy.set(false));
  }

  /** Sign in to a shop (one-time) so its search/detail calls return results. */
  connectShop(provider: ShopProvider): void {
    this.shops.connect(provider).then(
      () => this.feedback.notify(`Connected to ${provider.displayName}.`),
      () => this.feedback.error(`Could not connect to ${provider.displayName}.`),
    );
  }

  /** Fetch the shop product's detail, import it into the catalog (the server
   *  caches the image), and close linked to the imported row. */
  pickShop(provider: ShopProvider, candidate: ShopCandidate): void {
    if (this.importing()) return;
    this.importing.set(true);
    this.shops
      .fetchProduct(provider, candidate.external_id)
      .then((p) =>
        firstValueFrom(
          this.api.importProduct({
            source: p.source,
            external_id: p.external_id,
            name: p.name ?? candidate.name,
            brand: p.brand,
            image_url: p.image_url,
          }),
        ),
      )
      .then((product) =>
        this.ref.close({
          name: product.name ?? candidate.name,
          barcode: product.barcode,
          product_id: product.id,
          unit: null,
          category: null,
        }),
      )
      .catch(() => this.feedback.error(`Could not link the ${provider.displayName} product.`))
      .finally(() => this.importing.set(false));
  }

  close(): void {
    this.ref.close(null);
  }
}
