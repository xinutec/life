import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  MAT_BOTTOM_SHEET_DATA,
  MatBottomSheetRef,
} from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog, MatDialogModule } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatMenuModule } from '@angular/material/menu';
import { MatSelectModule } from '@angular/material/select';

import { isNotFound, onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { Item, ItemCategory } from '../../models';
import { ShopCandidate, ShopProvider, Shops } from '../../shop';
import { WAITROSE } from '../../shops/waitrose';
import { ScannerDialog } from '../scanner/scanner-dialog';
import { ShopSearchData, ShopSearchDialog } from './shop-search-dialog';

const CATEGORIES: ItemCategory[] = ['food', 'medication', 'tool', 'document', 'other'];

/** Shops offered for enrichment. Adding one (Asda) is a single entry here plus
 *  its `shops/<shop>.ts` provider — no other change in this sheet. */
const PROVIDERS: ShopProvider[] = [WAITROSE];

export interface ItemSheetData {
  /** Present = edit; absent = add. */
  item?: Item;
  /** Location dropdown options, already resolved by the parent. */
  locations: { id: number; label: string }[];
}

interface ItemForm {
  name: string;
  category: ItemCategory;
  quantity: number | null;
  unit: string | null;
  expiry: string | null;
  location_id: number | null;
  barcode: string | null;
  /** Set when linked to a catalog product (incl. a barcodeless shop product). */
  product_id: number | null;
}

/** Add/edit an inventory item — the FAB's bottom sheet. Online-only (the
 *  inventory is a server API, not a sync store); dismisses with `true` after a
 *  successful save so the parent reloads. */
@Component({
  selector: 'app-item-sheet',
  templateUrl: './item-sheet.html',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatMenuModule,
    MatSelectModule,
    SheetHeader,
  ],
})
export class ItemSheet {
  private ref = inject(MatBottomSheetRef<ItemSheet, boolean>);
  private data = inject<ItemSheetData>(MAT_BOTTOM_SHEET_DATA);
  private api = inject(LifeApi);
  private dialog = inject(MatDialog);
  private feedback = inject(Feedback);
  private shops = inject(Shops);

  readonly categories = CATEGORIES;
  readonly locations = this.data.locations;
  readonly editing = this.data.item != null;
  readonly saving = signal(false);
  /** Shop enrichment only works inside the Android app (needs the native bridge). */
  readonly shopProviders = this.shops.available ? PROVIDERS : [];

  readonly form = signal<ItemForm>(
    this.data.item
      ? {
          name: this.data.item.name,
          category: this.data.item.category,
          quantity: this.data.item.quantity,
          unit: this.data.item.unit,
          expiry: this.data.item.expiry,
          location_id: this.data.item.location_id,
          barcode: this.data.item.barcode,
          product_id: this.data.item.product_id,
        }
      : {
          name: '',
          category: 'food',
          quantity: null,
          unit: null,
          expiry: null,
          location_id: null,
          barcode: null,
          product_id: null,
        },
  );
  patch(p: Partial<ItemForm>): void {
    this.form.update((f) => ({ ...f, ...p }));
  }

  save(): void {
    if (!this.form().name.trim() || this.saving()) return;
    this.saving.set(true);
    const body = { ...this.form() };
    const id = this.data.item?.id;
    const req = id != null ? this.api.updateItem(id, body) : this.api.createItem(body);
    const trimmed = this.form().barcode?.trim();
    const barcode = trimmed !== undefined && trimmed !== '' ? trimmed : null;
    req.subscribe({
      next: () => {
        // Cache the product image (if a barcode was set) before the parent
        // refreshes — best-effort, the dismissal doesn't wait for it.
        if (barcode) this.api.lookupProduct(barcode).subscribe({ next: () => {}, error: () => {} });
        this.ref.dismiss(true);
      },
      error: (e: unknown) => {
        this.saving.set(false);
        this.feedback.error(`Could not save the item${onlineHint(e)}`);
      },
    });
  }

  /** Scan a barcode into the form; look up to cache + prefill the name.
   *  Every outcome is announced — a scan that ends in silence reads as "the
   *  scanner is broken". */
  scan(): void {
    this.dialog
      .open<ScannerDialog, unknown, string | null>(ScannerDialog, {
        panelClass: 'scanner-pane',
        ariaLabel: 'Barcode scanner',
      })
      .afterClosed()
      .subscribe((code) => {
        if (!code) return;
        this.patch({ barcode: code });
        this.api.lookupProduct(code).subscribe({
          next: (p) => {
            if (!this.form().name.trim() && p.name) this.patch({ name: p.name });
            this.feedback.notify(p.name ? `Found: ${p.name}` : 'Product found');
          },
          error: (e: unknown) => {
            this.feedback.error(
              isNotFound(e)
                ? `No product found for ${code}.`
                : 'Lookup failed — are you online?',
            );
          },
        });
      });
  }

  /** Sign in to a shop (one-time) so its search/detail calls return results.
   *  The native layer shows the shop's own login page; cookies then persist. */
  connectShop(provider: ShopProvider): void {
    this.shops.connect(provider).then(
      () => this.feedback.notify(`Connected to ${provider.displayName}.`),
      () => this.feedback.error(`Could not connect to ${provider.displayName}.`),
    );
  }

  /** Search a shop by name, pick a product, then fetch its detail, import it into
   *  the catalog, and link this item to it (prefilling the name). */
  findOnShop(provider: ShopProvider): void {
    this.dialog
      .open<ShopSearchDialog, ShopSearchData, ShopCandidate | null>(ShopSearchDialog, {
        data: { provider, initialQuery: this.form().name.trim() },
        ariaLabel: `Find on ${provider.displayName}`,
      })
      .afterClosed()
      .subscribe((candidate) => {
        if (candidate) this.importCandidate(provider, candidate);
      });
  }

  /** Fetch the full shop product, import it (server caches the image), and link. */
  private importCandidate(provider: ShopProvider, candidate: ShopCandidate): void {
    this.saving.set(true);
    this.shops
      .fetchProduct(provider, candidate.external_id)
      .then(
        (p) =>
          new Promise<void>((resolve, reject) =>
            this.api
              .importProduct({
                source: p.source,
                external_id: p.external_id,
                name: p.name ?? candidate.name,
                brand: p.brand,
                image_url: p.image_url,
              })
              .subscribe({
                next: (product) => {
                  // The catalog resolves the display name from the product, but
                  // fill an empty field so the user sees what they linked.
                  const typed = this.form().name.trim();
                  this.patch({
                    product_id: product.id,
                    name: typed !== '' ? typed : (product.name ?? candidate.name),
                  });
                  this.feedback.notify(`Linked to ${product.name ?? candidate.name}.`);
                  resolve();
                },
                error: reject,
              }),
          ),
      )
      .catch(() =>
        this.feedback.error(`Could not link the ${provider.displayName} product.`),
      )
      .finally(() => this.saving.set(false));
  }

  close(): void {
    this.ref.dismiss();
  }
}
