import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { localDay } from '../../shared/civil-day';
import { Dialog } from '../../shared/dialog';
import { Feedback } from '../../shared/feedback';
import { onlineHint } from '../../shared/api-error';
import { toMinorUnits } from '../../shared/money';
import { LifeApi } from '../../life-api';
import { Item, Purchase } from '../../models';

export interface PurchaseDialogData {
  item: Item;
}

/** Record what something already owned cost; a warranty is measured from it. */
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
  /** Empty means today. */
  readonly boughtOn = signal('');
  /** Empty is "none recorded", not "no warranty". */
  readonly warranty = signal('');

  /** Pence, or null; the button is disabled without a price. */
  readonly pence = computed(() => toMinorUnits(this.price()));

  /** Whole months only: "2.5" is refused, not rounded. */
  readonly months = computed(() => {
    const raw = this.warranty().trim();
    if (!raw) return null;
    return /^\d+$/.test(raw) ? Number(raw) : null;
  });

  readonly warrantyBad = computed(() => this.warranty().trim() !== '' && this.months() === null);

  readonly canSave = computed(
    () =>
      !this.saving() && this.shop().trim() !== '' && this.pence() !== null && !this.warrantyBad(),
  );

  /** The latest a purchase can be. */
  readonly today = localDay();

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
