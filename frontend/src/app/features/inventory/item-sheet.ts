import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatSelectModule } from '@angular/material/select';
import { Dialogs, Sheets } from '@xinutec/ui-scaffold';

import { isNotFound, onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ProductPick, ProductPickData, ProductPicker } from '../../shared/product-picker';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { monthEnd, toMonth } from '../../expiry';
import {
  ExpiryPrecision,
  ITEM_CATEGORIES,
  ITEM_CATEGORY_LABEL,
  Item,
  ItemCategory,
} from '../../models';
import { ScannerDialog } from '../scanner/scanner-dialog';
import { HistoryDialog, HistoryDialogData } from './history-dialog';
import { FilesDialog, FilesDialogData } from './files-dialog';
import { PurchaseDialog, PurchaseDialogData } from './purchase-dialog';

export interface ItemSheetData {
  /** Absent when adding. */
  item?: Item;
  locations: { id: number; label: string }[];
}

interface ItemForm {
  name: string;
  category: ItemCategory;
  quantity: number | null;
  unit: string | null;
  expiry: string | null;
  expiry_precision: ExpiryPrecision;
  location_id: number | null;
  barcode: string | null;
  product_id: number | null;
}

/** Add or edit an inventory item; dismisses with `true` after a save. */
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
    MatButtonToggleModule,
    MatSelectModule,
    SheetHeader,
  ],
})
export class ItemSheet {
  private ref = inject(MatBottomSheetRef<ItemSheet, boolean>);
  private sheets = inject(Sheets);
  private data = inject<ItemSheetData>(MAT_BOTTOM_SHEET_DATA);
  private api = inject(LifeApi);
  private dialog = inject(Dialogs);
  private feedback = inject(Feedback);

  readonly categories = ITEM_CATEGORIES;
  label(c: ItemCategory): string {
    return ITEM_CATEGORY_LABEL[c];
  }
  readonly locations = this.data.locations;
  readonly editing = this.data.item != null;
  readonly saving = signal(false);

  readonly form = signal<ItemForm>(
    this.data.item
      ? {
          name: this.data.item.name,
          category: this.data.item.category,
          quantity: this.data.item.quantity,
          unit: this.data.item.unit,
          expiry: this.data.item.expiry,
          expiry_precision: this.data.item.expiry_precision,
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
          expiry_precision: 'day',
          location_id: null,
          barcode: null,
          product_id: null,
        },
  );
  patch(p: Partial<ItemForm>): void {
    this.form.update((f) => ({ ...f, ...p }));
  }

  /** With no expiry yet, medication defaults to month precision (MM/YYYY packs). */
  chooseCategory(category: ItemCategory): void {
    if (this.form().expiry != null) {
      this.patch({ category });
      return;
    }
    this.patch({ category, expiry_precision: category === 'medication' ? 'month' : 'day' });
  }

  readonly expiryMonth = computed(() => toMonth(this.form().expiry));

  /** Stored as the month's LAST day: 06/2028 is good through June. */
  setExpiryMonth(month: string | null): void {
    this.patch({ expiry: month ? monthEnd(month) : null });
  }

  /** Switching precision keeps the date rather than dropping it. */
  setPrecision(expiry_precision: ExpiryPrecision): void {
    const expiry = this.form().expiry;
    this.patch({
      expiry_precision,
      expiry: expiry_precision === 'month' && expiry ? monthEnd(toMonth(expiry) ?? '') : expiry,
    });
  }

  /** Whether the person typed this name: theirs outranks the catalogue's. False
   *  for a prefilled name, so saving unchanged changes nothing. */
  private readonly nameIsMine = signal(false);

  renameByHand(name: string): void {
    this.nameIsMine.set(true);
    this.patch({ name });
  }

  /** A name from the catalogue, which later catalogue corrections may update. */
  private nameFromCatalog(name: string): void {
    this.nameIsMine.set(false);
    this.patch({ name });
  }

  save(): void {
    if (!this.form().name.trim() || this.saving()) return;
    this.saving.set(true);
    // Absent unless typed: `null` would claim the name is the catalogue's.
    const body = {
      ...this.form(),
      ...(this.nameIsMine() ? { name_source: 'user' as const } : {}),
    };
    const id = this.data.item?.id;
    const req = id != null ? this.api.updateItem(id, body) : this.api.createItem(body);
    const trimmed = this.form().barcode?.trim();
    const barcode = trimmed !== undefined && trimmed !== '' ? trimmed : null;
    req.subscribe({
      next: () => {
        // Warms the product image cache; best-effort.
        if (barcode) this.api.lookupProduct(barcode).subscribe({ next: () => {}, error: () => {} });
        this.ref.dismiss(true);
      },
      error: (e: unknown) => {
        this.saving.set(false);
        this.feedback.error(`Could not save the item${onlineHint(e)}`);
      },
    });
  }

  /** Every outcome is announced: silence reads as a broken scanner. */
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
            if (!this.form().name.trim() && p.name) this.nameFromCatalog(p.name);
            this.feedback.notify(p.name ? `Found: ${p.name}` : 'Product found');
          },
          error: (e: unknown) => {
            this.feedback.error(
              isNotFound(e) ? `No product found for ${code}.` : 'Lookup failed — are you online?',
            );
          },
        });
      });
  }

  findProduct(): void {
    this.dialog
      .open<ProductPicker, ProductPickData, ProductPick | null>(ProductPicker, {
        data: { initialQuery: this.form().name.trim() },
        ariaLabel: 'Find a product',
      })
      .afterClosed()
      .subscribe((pick) => {
        if (!pick) return;
        this.patch({ barcode: pick.barcode, product_id: pick.product_id });
        this.nameFromCatalog(pick.name);
        if (pick.unit != null && !this.form().unit?.trim()) this.patch({ unit: pick.unit });
        // A new row starts at its pack size, unless an amount is already typed.
        if (pick.quantity != null && this.form().quantity == null) {
          this.patch({ quantity: pick.quantity });
        }
      });
  }

  readonly canViewProduct = computed(
    () => this.form().product_id != null || !!this.form().barcode?.trim(),
  );

  /** Open the product's page, looking a barcode up first. */
  viewProduct(): void {
    const pid = this.form().product_id;
    if (pid != null) {
      void this.sheets.dismissTo(this.ref, ['/product', pid]);
      return;
    }
    const barcode = this.form().barcode?.trim();
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

  /** A dialog, not a second bottom sheet, which would dismiss this form. */
  viewHistory(): void {
    const item = this.data.item;
    if (!item) return;
    this.dialog.open<HistoryDialog, HistoryDialogData, void>(HistoryDialog, {
      data: { item },
      ariaLabel: `History of ${item.name}`,
    });
  }

  recordPurchase(): void {
    const item = this.data.item;
    if (!item) return;
    this.dialog.open<PurchaseDialog, PurchaseDialogData, unknown>(PurchaseDialog, {
      data: { item },
      ariaLabel: `Record what ${item.name} cost`,
    });
  }

  viewFiles(): void {
    const item = this.data.item;
    if (!item) return;
    this.dialog.open<FilesDialog, FilesDialogData, void>(FilesDialog, {
      data: { item },
      ariaLabel: `Files for ${item.name}`,
    });
  }

  close(): void {
    this.ref.dismiss();
  }
}
