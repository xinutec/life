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

/** Shares of what is on hand: "half the bag", not "475 g". */
const SHARES = [
  { label: '¼', of: 0.25 },
  { label: '½', of: 0.5 },
  { label: 'All of it', of: 1 },
];

/** "I used some of this", always in the row's own unit: the backend never
 *  converts. Dismisses with `true` after a save. */
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

  readonly unit = this.item.unit ?? '';

  private how(quantity: number): string {
    return amount(quantity, this.item.unit);
  }

  readonly have = this.item.quantity ?? 0;

  readonly haveLabel = amount(this.have, this.item.unit);

  readonly shares = computed(() =>
    this.have > 0 ? SHARES.map((s) => ({ label: s.label, amount: round(this.have * s.of) })) : [],
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
        const short = round(quantity - this.have);
        this.feedback.notify(
          left > 0
            ? `Used ${this.how(quantity)} — ${this.how(round(left))} left.`
            : this.have > 0 && short > 0
              ? `Used all ${this.how(this.have)} — ${this.how(short)} more than was recorded.`
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

function round(n: number): number {
  return Math.round(n * 100) / 100;
}
