import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';

import { SheetHeader } from '../../shared/sheet-header';
import { toMinorUnits } from '../../shared/money';

/** One ticked row, as this sheet needs to show it. */
export interface BuyRow {
  id: number;
  name: string;
}

/** What the sheet hands back: the shop, and the prices that were filled in.
 *
 *  `prices` holds only the rows that got one — an empty box is not a price of
 *  zero, and recording it as one would put a free item in the spending history. */
export interface BuyPrices {
  shop: string;
  prices: Map<number, number>;
}

/** Where the last shop is remembered. Local, not server: it is a convenience for
 *  the next trip, not a fact worth syncing, and it must survive being offline in
 *  a shop — which is exactly where this sheet is used. */
const LAST_SHOP_KEY = 'life.lastShop';

/**
 * Ask where the shopping happened and what each thing cost, on the way to
 * putting it in the cupboard.
 *
 * Everything here is optional, deliberately. Marking things bought is the
 * gesture that empties the list, and it happens standing in a kitchen with bags
 * to unpack — a step that demands a price for each row before it will let the
 * list empty would be dismissed once and then routed around forever. So "Add
 * without prices" is a first-class button and not a cancel.
 */
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
  /** Raw text per row id — kept as typed so a half-entered "3." is not thrown
   *  away mid-keystroke, and only read as money when the sheet is submitted. */
  readonly typed = signal<Record<number, string>>({});

  /** The text typed for a row, or ''. A method rather than indexing in the
   *  template: `noUncheckedIndexedAccess` is off (see #138), so the index type
   *  is not `| undefined` and the template's `?? ''` reads as dead code to the
   *  compiler while being load-bearing at runtime. */
  priceText(id: number): string {
    return this.typed()[id] ?? '';
  }

  setPrice(id: number, text: string): void {
    this.typed.update((t) => ({ ...t, [id]: text }));
  }

  /** Rows whose text is present but unreadable as a price. Shown rather than
   *  silently dropped: a typo that vanishes looks exactly like a price that was
   *  recorded. */
  readonly unreadable = computed(() =>
    this.rows.filter((r) => {
      const text = this.priceText(r.id);
      return text.trim() !== '' && toMinorUnits(text) === null;
    }),
  );

  /** The offending rows, named, for a message that says which one to fix. */
  readonly unreadableNames = computed(() => this.unreadable().map((r) => r.name).join(', '));

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

  /** Buy them without recording anything — the old behaviour, kept reachable. */
  skip(): void {
    this.ref.dismiss('skip');
  }

  /** Dismissing the sheet buys nothing: closing something you opened by mistake
   *  must not empty the list. */
  close(): void {
    this.ref.dismiss(undefined);
  }
}
