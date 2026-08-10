import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { amount } from '../../shared/amount';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { Item } from '../../models';

export interface UseSheetData {
  item: Item;
}

/** Fractions of what's on hand, because that is how you actually think about it
 *  — "half the bag" rather than "475 g". Computed from the row, so the numbers
 *  are always real amounts of a real thing. */
const SHARES = [
  { label: '¼', of: 0.25 },
  { label: '½', of: 0.5 },
  { label: 'All of it', of: 1 },
];

/** "I used some of this" — bottom sheet over an inventory row.
 *
 *  Deliberately does NOT let you pick a unit: the amount is always in the row's
 *  own unit, because a conversion is exactly what the backend refuses to guess
 *  (see inventory::consume). What you type is what comes off.
 *
 *  Dismisses with `true` after a successful decrement so the parent reloads. */
@Component({
  selector: 'app-use-sheet',
  templateUrl: './use-sheet.html',
  styleUrl: './use-sheet.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    SheetHeader,
  ],
})
export class UseSheet {
  private ref = inject(MatBottomSheetRef<UseSheet, boolean>);
  private data = inject<UseSheetData>(MAT_BOTTOM_SHEET_DATA);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);

  readonly item = this.data.item;
  readonly saving = signal(false);
  readonly amount = signal<number | null>(null);

  /** The unit shown as the field's suffix; blank for countable things. */
  readonly unit = this.item.unit ?? '';

  /** An amount written the way the rest of the app writes one — "950 g", but
   *  "1 bottle" rather than "1bottle". This sheet used to concatenate them, so
   *  every word-unit row read as a typo. */
  private how(quantity: number): string {
    return amount(quantity, this.item.unit);
  }

  /** What's on hand, for the share buttons' arithmetic. */
  readonly have = this.item.quantity ?? 0;

  /** The same, written out for the "you have N" line. */
  readonly haveLabel = amount(this.have, this.item.unit);

  readonly shares = computed(() =>
    this.have > 0
      ? SHARES.map((s) => ({ label: s.label, amount: round(this.have * s.of) }))
      : [],
  );

  readonly valid = computed(() => {
    const n = this.amount();
    return n !== null && Number.isFinite(n) && n > 0;
  });

  pick(amount: number): void {
    this.amount.set(amount);
  }

  save(): void {
    const quantity = this.amount();
    if (quantity === null || !this.valid() || this.saving()) return;
    this.saving.set(true);
    this.api.useItem(this.item.id, quantity, this.item.unit).subscribe({
      next: (updated) => {
        const left = updated.quantity ?? 0;
        this.feedback.notify(
          left > 0
            ? `Used ${this.how(quantity)} — ${this.how(round(left))} left.`
            : `Used the last of the ${this.item.name.toLowerCase()}.`,
        );
        this.ref.dismiss(true);
      },
      error: (e: unknown) => {
        this.saving.set(false);
        this.feedback.error(`Could not record that${onlineHint(e)}`);
      },
    });
  }

  close(): void {
    this.ref.dismiss();
  }
}

/** Two decimals at most — a third of a 950 g bag is 316.666…, and nobody has
 *  that much flour. */
function round(n: number): number {
  return Math.round(n * 100) / 100;
}
