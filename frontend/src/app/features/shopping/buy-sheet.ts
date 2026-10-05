import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';

import { SheetHeader } from '../../shared/sheet-header';
import { toMinorUnits } from '../../shared/money';

export interface BuyRow {
  id: number;
  name: string;
}

/** The shop, and only the prices filled in: an empty box is not a price of 0. */
export interface BuyPrices {
  shop: string;
  prices: Map<number, number>;
}

/** Remembered on the device, which is offline in a shop. */
const LAST_SHOP_KEY = 'life.lastShop';

/** Where it was bought and what it cost, all optional: a step that demanded
 *  prices while unpacking would be routed around. */
@Component({
  selector: 'app-buy-sheet',
  templateUrl: './buy-sheet.html',
  styleUrl: './buy-sheet.scss',
  imports: [FormsModule, MatButtonModule, MatFormFieldModule, MatInputModule, SheetHeader],
})
export class BuySheet {
  private ref = inject(MatBottomSheetRef<BuySheet, BuyPrices | 'skip' | undefined>);
  readonly rows = inject<BuyRow[]>(MAT_BOTTOM_SHEET_DATA);

  readonly shop = signal(localStorage.getItem(LAST_SHOP_KEY) ?? '');
  /** As typed, so a half-entered "3." survives until submit. */
  readonly typed = signal<Record<number, string>>({});

  /** A method: `?? ''` in the template would read as dead code. */
  priceText(id: number): string {
    return this.typed()[id] ?? '';
  }

  setPrice(id: number, text: string): void {
    this.typed.update((t) => ({ ...t, [id]: text }));
  }

  /** Typed but not a price; shown, as a dropped typo looks recorded. */
  readonly unreadable = computed(() =>
    this.rows.filter((r) => {
      const text = this.priceText(r.id);
      return text.trim() !== '' && toMinorUnits(text) === null;
    }),
  );

  readonly unreadableNames = computed(() =>
    this.unreadable()
      .map((r) => r.name)
      .join(', '),
  );

  readonly canRecord = computed(() => this.shop().trim() !== '' && this.unreadable().length === 0);

  record(): void {
    if (!this.canRecord()) return;
    const shop = this.shop().trim();
    localStorage.setItem(LAST_SHOP_KEY, shop);
    const prices = new Map<number, number>();
    for (const r of this.rows) {
      const minor = toMinorUnits(this.priceText(r.id));
      if (minor !== null) prices.set(r.id, minor);
    }
    this.ref.dismiss({ shop, prices });
  }

  skip(): void {
    this.ref.dismiss('skip');
  }

  /** Closing buys nothing. */
  close(): void {
    this.ref.dismiss(undefined);
  }
}
