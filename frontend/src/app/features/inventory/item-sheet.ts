import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import {
  MAT_BOTTOM_SHEET_DATA,
  MatBottomSheetRef,
} from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog, MatDialogModule } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatSelectModule } from '@angular/material/select';

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
  /** Which input the expiry is typed into, and what gets saved with it. */
  expiry_precision: ExpiryPrecision;
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
    MatButtonToggleModule,
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
  private router = inject(Router);

  readonly categories = ITEM_CATEGORIES;
  /** Its display name — the picker shows what a category is called, not its key. */
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

  /**
   * A category picked while the expiry box is still empty also picks how that
   * box asks the question.
   *
   * Medicine is printed MM/YYYY and nothing else, so a date picker makes
   * somebody choose a day that is not on the packet; everything else in a
   * cupboard carries a full date. Only while the field is UNTOUCHED — a date
   * already typed is an answer, and re-categorising must not quietly widen it.
   */
  chooseCategory(category: ItemCategory): void {
    if (this.form().expiry != null) {
      this.patch({ category });
      return;
    }
    this.patch({ category, expiry_precision: category === 'medication' ? 'month' : 'day' });
  }

  /** `YYYY-MM` for the month input — the stored date is that month's last day. */
  readonly expiryMonth = computed(() => toMonth(this.form().expiry));

  /** A month picked in the month input, stored as the month's LAST day: a box
   *  marked 06/2028 is good THROUGH June, and the 1st would expire it early. */
  setExpiryMonth(month: string | null): void {
    this.patch({ expiry: month ? monthEnd(month) : null });
  }

  /** Switch how the expiry is asked for. Re-reads the date through the new
   *  precision rather than dropping it: month → day keeps the month-end as a
   *  starting point to correct, and day → month keeps the month it fell in. */
  setPrecision(expiry_precision: ExpiryPrecision): void {
    const expiry = this.form().expiry;
    this.patch({
      expiry_precision,
      expiry: expiry_precision === 'month' && expiry ? monthEnd(toMonth(expiry) ?? '') : expiry,
    });
  }

  /**
   * Whether the person typed this name themselves.
   *
   * The server cannot tell. It sees a name and a linked product and nothing
   * about how the name got there, so it has to be told: an item whose name is
   * the person's outranks the catalogue and stops following it, and one that is
   * not keeps taking corrections forever. Only this form knows which happened,
   * because only this form sees the keystroke.
   *
   * Starts false even when editing: the box is prefilled with the name the item
   * already DISPLAYS, so opening the sheet and saving must change nothing.
   */
  private readonly nameIsMine = signal(false);

  /** The name box changed because somebody typed in it. */
  renameByHand(name: string): void {
    this.nameIsMine.set(true);
    this.patch({ name });
  }

  /** A name that arrived FROM the catalogue — a scan or a product pick. Hands
   *  the name back to the product, so later corrections reach this item. */
  private nameFromCatalog(name: string): void {
    this.nameIsMine.set(false);
    this.patch({ name });
  }

  save(): void {
    if (!this.form().name.trim() || this.saving()) return;
    this.saving.set(true);
    // Absent unless it is a statement: `null` here would be a claim that the
    // name is the catalogue's, and a plain save must not make that claim on
    // somebody's behalf (the server preserves what the item already had).
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
            if (!this.form().name.trim() && p.name) this.nameFromCatalog(p.name);
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

  /** Name-first product search (the shared picker — inventory, catalog, and the
   *  shop tier inside the app); a pick links the item and fills the form. */
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
        // The pack size is how much this row holds — a 950g tub starts at 950g,
        // which is what makes "how much is left" answerable at all. Only when
        // the field is empty: a number already typed is a real measurement of
        // this row (half a tub), and the label is only what it held when new.
        if (pick.quantity != null && this.form().quantity == null) {
          this.patch({ quantity: pick.quantity });
        }
      });
  }

  /** Whether "View product" has somewhere to go: a linked product, or a barcode
   *  it can resolve to one. */
  readonly canViewProduct = computed(
    () => this.form().product_id != null || !!this.form().barcode?.trim(),
  );

  /** Leave the sheet for the linked product's page — the scan → payoff-screen
   *  path. A barcode without an established link is looked up first. */
  viewProduct(): void {
    const pid = this.form().product_id;
    if (pid != null) {
      this.ref.dismiss();
      void this.router.navigate(['/product', pid]);
      return;
    }
    const barcode = this.form().barcode?.trim();
    if (!barcode) return;
    this.api.lookupProduct(barcode).subscribe({
      next: (p) => {
        this.ref.dismiss();
        void this.router.navigate(['/product', p.id]);
      },
      error: (e: unknown) => {
        this.feedback.error(
          isNotFound(e) ? `No product found for ${barcode}.` : 'Lookup failed — are you online?',
        );
      },
    });
  }

  /** Everything that has happened to this row. A dialog OVER the sheet, not a
   *  second bottom sheet — Material holds one of those at a time, so opening
   *  one here would dismiss this form and lose whatever had been typed into it. */
  viewHistory(): void {
    const item = this.data.item;
    if (!item) return;
    this.dialog.open<HistoryDialog, HistoryDialogData, void>(HistoryDialog, {
      data: { item },
      ariaLabel: `History of ${item.name}`,
    });
  }

  /** Record what this item cost — a dialog, for the same reason `viewHistory`
   *  is one: a second bottom sheet would dismiss this form under it. */
  recordPurchase(): void {
    const item = this.data.item;
    if (!item) return;
    this.dialog.open<PurchaseDialog, PurchaseDialogData, unknown>(PurchaseDialog, {
      data: { item },
      ariaLabel: `Record what ${item.name} cost`,
    });
  }

  /** Receipts and manuals for this item — a dialog, same reason as the others. */
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
