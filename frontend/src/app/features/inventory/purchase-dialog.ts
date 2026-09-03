import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { Dialog } from '../../shared/dialog';
import { Feedback } from '../../shared/feedback';
import { onlineHint } from '../../shared/api-error';
import { toMinorUnits } from '../../shared/money';
import { LifeApi } from '../../life-api';
import { Item, Purchase } from '../../models';

export interface PurchaseDialogData {
  item: Item;
}

/**
 * Record what something cost, for something already owned.
 *
 * The buy-list flow writes a purchase when you tick a row off in a shop, and it
 * was the only thing that did — so a dishwasher, a pan, anything acquired before
 * the app existed had no way to carry a price or a date. That matters beyond the
 * money: a warranty is measured FROM a purchase, so without one there is nothing
 * to count from.
 *
 * A dialog over the item sheet, not a second bottom sheet: Material holds one
 * sheet at a time and a second would dismiss the edit form under it, taking any
 * unsaved typing with it. Same reason the history dialog and the product picker
 * are dialogs.
 */
@Component({
  selector: 'app-purchase-dialog',
  templateUrl: './purchase-dialog.html',
  styleUrl: './purchase-dialog.scss',
  imports: [
    Dialog,
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
  ],
})
export class PurchaseDialog {
  private ref = inject(MatDialogRef<PurchaseDialog, Purchase | undefined>);
  private data = inject<PurchaseDialogData>(MAT_DIALOG_DATA);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);

  readonly item = this.data.item;
  readonly saving = signal(false);

  readonly shop = signal('');
  readonly price = signal('');
  /** `YYYY-MM-DD`. Empty means today, which is what the server reads an absent
   *  date as — the shop case, where you are standing at the till. */
  readonly boughtOn = signal('');
  /** Months, as printed on the receipt. Empty is "no warranty recorded", which
   *  is not a claim that there is none. */
  readonly warranty = signal('');

  /** Pence, or null when the box does not hold a price.
   *
   *  Parsed here rather than at save so the button can be disabled on it: a
   *  price is the one field with no sensible fallback, and a purchase of nothing
   *  is not a record worth keeping. Integer arithmetic throughout — `3.30 * 100`
   *  is 330.00000000000006, and money must be exact. */
  readonly pence = computed(() => toMinorUnits(this.price()));

  /** Whole months, or null. Rejects a decimal outright rather than rounding it:
   *  "2.5 years" typed into a months box is a misunderstanding, and silently
   *  storing 2 months of cover would be worse than refusing. */
  readonly months = computed(() => {
    const raw = this.warranty().trim();
    if (!raw) return null;
    return /^\d+$/.test(raw) ? Number(raw) : null;
  });

  /** Whether the warranty box holds something that is not a number of months.
   *  Distinguished from empty, because empty is fine and wrong is not. */
  readonly warrantyBad = computed(() => this.warranty().trim() !== '' && this.months() === null);

  readonly canSave = computed(
    () => !this.saving() && this.shop().trim() !== '' && this.pence() !== null && !this.warrantyBad(),
  );

  /** Today, as the date input spells it — the latest a purchase can be, since
   *  the server refuses one that has not happened. */
  readonly today = new Date().toLocaleDateString('en-CA');

  save(): void {
    const amount = this.pence();
    if (!this.canSave() || amount === null) return;
    this.saving.set(true);
    this.api
      .recordPurchase(this.item.id, {
        shop: this.shop().trim(),
        amount_minor: amount,
        currency: 'GBP',
        bought_on: this.boughtOn() || null,
        warranty_months: this.months(),
      })
      .subscribe({
        next: (p) => this.ref.close(p),
        error: (e: unknown) => {
          this.saving.set(false);
          this.feedback.error(`Could not record the purchase${onlineHint(e)}`);
        },
      });
  }

  close(): void {
    this.ref.close();
  }
}
