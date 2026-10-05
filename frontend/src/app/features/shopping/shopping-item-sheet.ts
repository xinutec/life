import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatSelectModule } from '@angular/material/select';
import { Dialogs, Sheets } from '@xinutec/ui-scaffold';

import { isNotFound } from '../../shared/api-error';
import { ITEM_CATEGORIES, ITEM_CATEGORY_LABEL, ItemCategory } from '../../models';
import { Feedback } from '../../shared/feedback';
import { ProductPick, ProductPickData, ProductPicker } from '../../shared/product-picker';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { ScannerDialog } from '../scanner/scanner-dialog';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { canonicalBarcode } from '../../shared/barcode';

/** Add or edit a Buy row. Adding stays open for the next item. */
@Component({
  selector: 'app-shopping-item-sheet',
  templateUrl: './shopping-item-sheet.html',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatSelectModule,
    SheetHeader,
  ],
})
export class ShoppingItemSheet {
  private ref = inject(MatBottomSheetRef<ShoppingItemSheet>);
  private sheets = inject(Sheets);
  private data = inject<{ ulid?: string } | null>(MAT_BOTTOM_SHEET_DATA, { optional: true });
  private store = inject(ShoppingStore);
  private api = inject(LifeApi);
  private dialog = inject(Dialogs);
  private feedback = inject(Feedback);

  private items = toSignal(this.store.items$, { initialValue: [] as ShoppingDoc[] });

  readonly ulid = this.data?.ulid ?? null;
  readonly editing = this.ulid != null;

  readonly categories = ITEM_CATEGORIES;
  label(c: ItemCategory): string {
    return ITEM_CATEGORY_LABEL[c];
  }

  readonly name = signal('');
  readonly quantity = signal<number | null>(null);
  readonly unit = signal<string | null>(null);
  readonly barcode = signal('');
  /** Carried onto the inventory item when bought. */
  readonly category = signal<ItemCategory>('food');
  readonly productId = signal<number | null>(null);
  readonly lookingUp = signal(false);

  constructor() {
    if (this.ulid) {
      const it = this.items().find((i) => i.ulid === this.ulid);
      if (it) {
        this.name.set(it.name);
        this.quantity.set(it.quantity);
        this.unit.set(it.unit);
        this.barcode.set(it.barcode ?? '');
        // A category from an older device may have no option in the picker.
        const stored = ITEM_CATEGORIES.find((c) => c === it.category);
        if (stored !== undefined) this.category.set(stored);
        this.productId.set(it.product_id);
      }
    }
  }

  save(): void {
    const name = this.name().trim();
    if (!name) return;
    const unit = this.unit()?.trim();
    const barcode = canonicalBarcode(this.barcode()) || null;
    const fields = {
      name,
      quantity: this.quantity(),
      unit: unit !== undefined && unit !== '' ? unit : null,
      barcode,
      category: this.category(),
      product_id: this.productId(),
    };
    // dev-lint: allow-ignored-error warms the product image cache; best-effort
    if (barcode) this.api.lookupProduct(barcode).subscribe({ next: () => {}, error: () => {} });

    if (this.ulid) {
      void this.store.patch(this.ulid, fields);
      this.ref.dismiss();
      return;
    }
    void this.store.add(fields);
    // Putting a thing on the list says it is running out. Best-effort.
    this.api
      .markLowByIdentity({ name, barcode, product_id: this.productId() })
      // dev-lint: allow-ignored-error best-effort: putting it on the list is what was asked for
      .subscribe({ error: () => undefined });
    this.feedback.notify(`Added ${name}`);
    this.name.set('');
    this.quantity.set(null);
    this.unit.set(null);
    this.barcode.set('');
    this.category.set('food');
    this.productId.set(null);
    document.querySelector<HTMLElement>('app-shopping-item-sheet input')?.focus();
  }

  /** A hand-edited barcode drops the earlier lookup's catalogue link. */
  barcodeChanged(code: string): void {
    this.barcode.set(code);
    this.productId.set(null);
  }

  findProduct(): void {
    this.dialog
      .open<ProductPicker, ProductPickData, ProductPick | null>(ProductPicker, {
        data: { initialQuery: this.name().trim() },
        ariaLabel: 'Find a product',
      })
      .afterClosed()
      .subscribe((pick) => {
        if (!pick) return;
        this.name.set(pick.name);
        this.barcode.set(pick.barcode ?? '');
        this.productId.set(pick.product_id);
        if (pick.unit != null && !this.unit()?.trim()) this.unit.set(pick.unit);
        // Not `pick.quantity`: a 950 g tub is one to buy, not 950.
        if (pick.category != null) this.category.set(pick.category);
      });
  }

  scan(): void {
    this.dialog
      .open<ScannerDialog, unknown, string | null>(ScannerDialog, {
        panelClass: 'scanner-pane',
        ariaLabel: 'Barcode scanner',
      })
      .afterClosed()
      .subscribe((code) => {
        if (code) {
          this.barcode.set(code);
          this.lookup();
        }
      });
  }

  /** Every outcome is announced: silence reads as a broken scanner. */
  lookup(): void {
    const code = this.barcode().trim();
    if (!code) return;
    this.lookingUp.set(true);
    this.api.lookupProduct(code).subscribe({
      next: (p) => {
        this.lookingUp.set(false);
        this.productId.set(p.id);
        if (!this.name().trim() && p.name) this.name.set(p.name);
        this.feedback.notify(p.name ? `Found: ${p.name}` : 'Product found');
      },
      error: (e: unknown) => {
        this.lookingUp.set(false);
        this.feedback.error(
          isNotFound(e) ? `No product found for ${code}.` : 'Lookup failed — are you online?',
        );
      },
    });
  }

  readonly canViewProduct = computed(() => this.productId() != null || !!this.barcode().trim());

  /** Open the product's page, looking a barcode up first. */
  viewProduct(): void {
    const pid = this.productId();
    if (pid != null) {
      void this.sheets.dismissTo(this.ref, ['/product', pid]);
      return;
    }
    const barcode = this.barcode().trim();
    if (!barcode) return;
    this.api.lookupProduct(barcode).subscribe({
      next: (p) => {
        void this.sheets.dismissTo(this.ref, ['/product', p.id]);
      },
      error: (e: unknown) => {
        this.feedback.error(
          isNotFound(e) ? `No product found for ${barcode}.` : 'Lookup failed — are you online?',
        );
      },
    });
  }

  close(): void {
    this.ref.dismiss();
  }
}
