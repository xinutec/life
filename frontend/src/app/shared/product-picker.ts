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
import { AsdaHit, Item, ItemCategory, PackSize, Product } from '../models';
import { ShopCandidate, ShopProvider, Shops, shopPrice } from '../shop';
import { WAITROSE } from '../shops/waitrose';
import { ItemsStore } from '../stores/catalog';
import { Dialog } from './dialog';
import { Feedback } from './feedback';
import { sourceLabel } from './sources';

export interface ProductPickData {
  initialQuery: string;
}

/** What a pick fills into a form. `category` comes only from an inventory hit. */
export interface ProductPick {
  name: string;
  barcode: string | null;
  product_id: number | null;
  /** A pack size, not a number to buy; only the inventory sheet takes it. */
  quantity: number | null;
  unit: string | null;
  category: ItemCategory | null;
}

/** A `count` pack fills no unit: "3 eggs" is a bare number everywhere else. */
export function packPrefill(pack: PackSize | null): Pick<ProductPick, 'quantity' | 'unit'> {
  if (!pack) return { quantity: null, unit: null };
  return { quantity: pack.value, unit: pack.unit === 'count' ? null : pack.unit };
}

/** Inventory items matching the query, name-prefix hits first: the offline tier. */
export function localHits(items: Item[], query: string): Item[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const prefix = (it: Item) => (it.name.toLowerCase().startsWith(q) ? 0 : 1);
  return items
    .filter((it) => it.name.toLowerCase().includes(q) || (it.brand ?? '').toLowerCase().includes(q))
    .sort((a, b) => prefix(a) - prefix(b) || a.name.localeCompare(b.name))
    .slice(0, 8);
}

/** Catalogue rows not already shown as a local hit. */
export function withoutLocalDupes(catalog: Product[], locals: Item[]): Product[] {
  const ids = new Set(locals.map((it) => it.product_id).filter((v) => v != null));
  const codes = new Set(locals.map((it) => it.barcode).filter((v) => v != null));
  return catalog.filter((p) => !ids.has(p.id) && (p.barcode == null || !codes.has(p.barcode)));
}

/** A failed shop search usually means signed out. */
function shopMessage(e: unknown, provider: ShopProvider): string {
  const raw = e instanceof Error ? e.message : String(e);
  return /only available in the app/i.test(raw)
    ? raw
    : `Search failed — try “Connect ${provider.displayName}” first.`;
}

/** Shops the WebView searches in the shop tier. */
const PROVIDERS: ShopProvider[] = [WAITROSE];

/** Find a product: inventory (offline), catalogue, Asda, and in the app a shop
 *  search. Closes with a [[ProductPick]], or null. */
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

  readonly shopProviders = this.shops.available ? PROVIDERS : [];

  readonly query = signal(this.data.initialQuery.trim());
  private readonly query$ = new Subject<string>();

  /** A failed tier is an empty tier. */
  private readonly catalogRaw = toSignal(
    this.query$.pipe(
      debounceTime(250),
      distinctUntilChanged(),
      switchMap((q) =>
        q
          ? this.api.searchProducts(q).pipe(catchError(() => of([] as Product[])))
          : of([] as Product[]),
      ),
    ),
    { initialValue: [] as Product[] },
  );

  readonly asda = toSignal(
    this.query$.pipe(
      debounceTime(250),
      distinctUntilChanged(),
      switchMap((q) =>
        q
          ? this.api.searchAsda(q).pipe(catchError(() => of([] as AsdaHit[])))
          : of([] as AsdaHit[]),
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
    this.shopResults.set(null);
    this.shopError.set(null);
  }

  /** A plain <img>: [[ProductThumb]] would take the row's tap for itself. */
  thumbUrl(barcode: string | null, productId: number | null, hasImage: boolean): string | null {
    if (!hasImage) return null;
    if (barcode != null) return this.api.productImageUrl(barcode);
    if (productId != null) return this.api.productImageByIdUrl(productId);
    return null;
  }

  protected readonly sourceLabel = sourceLabel;

  pickItem(it: Item): void {
    this.ref.close({
      name: it.name,
      barcode: it.barcode,
      product_id: it.product_id,
      // Not its quantity: what is left of your tub says nothing about a new one.
      quantity: null,
      unit: it.unit,
      category: it.category,
    });
  }

  pickProduct(p: Product): void {
    this.ref.close({
      name: p.name ?? this.query(),
      barcode: p.barcode,
      product_id: p.id,
      ...packPrefill(p.pack),
      category: null,
    });
  }

  /** Import, then close linked to the imported product. */
  pickAsda(hit: AsdaHit): void {
    if (this.importing()) return;
    this.importing.set(true);
    firstValueFrom(
      this.api.importProduct({
        source: 'asda',
        external_id: hit.external_id,
        name: hit.name,
        brand: hit.brand,
        quantity_label: hit.quantity_label,
        barcode: hit.barcode,
        image_url: hit.image_url,
        price: hit.price,
      }),
    )
      .then((product) =>
        this.ref.close({
          name: product.name ?? hit.name,
          barcode: hit.barcode ?? product.barcode,
          product_id: product.id,
          ...packPrefill(product.pack),
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

  connectShop(provider: ShopProvider): void {
    this.shops.connect(provider).then(
      () => this.feedback.notify(`Connected to ${provider.displayName}.`),
      () => this.feedback.error(`Could not connect to ${provider.displayName}.`),
    );
  }

  /** Fetch, import, then close linked to the imported product. */
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
            quantity_label: p.quantity_label,
            barcode: p.barcodes[0] ?? null,
            image_url: p.image_url,
            price: shopPrice(p),
          }),
        ),
      )
      .then((product) =>
        this.ref.close({
          name: product.name ?? candidate.name,
          barcode: product.barcode,
          product_id: product.id,
          ...packPrefill(product.pack),
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
