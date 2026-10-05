import { Component, computed, inject, signal } from '@angular/core';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';

import { ago } from '../../shared/ago';
import { warrantyInfo } from '../../warranty';
import { amount } from '../../shared/amount';
import { formatMoney, formatUnitPrice } from '../../shared/money';
import { assertNever, classifyApiError, onlineHint } from '../../shared/api-error';
import { Dialog } from '../../shared/dialog';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { Item, ItemEvent, ItemHistoryEntry, Purchase } from '../../models';

export interface HistoryDialogData {
  item: Item;
}

interface Line {
  id: number;
  icon: string;
  /** With the amount when it is the verb's object ("Used 200 g"). */
  title: string;
  detail: string;
  when: string;
}

/** A total Record, so a new event kind cannot render as a blank row. */
const SHAPE: Record<ItemEvent, { icon: string; verb: string }> = {
  added: { icon: 'add_circle_outline', verb: 'Added' },
  moved: { icon: 'swap_horiz', verb: 'Moved' },
  used: { icon: 'remove_circle_outline', verb: 'Used' },
  removed: { icon: 'delete_outline', verb: 'Deleted' },
  restored: { icon: 'undo', verb: 'Restored' },
  low: { icon: 'add_shopping_cart', verb: 'Running low' },
};

/** Everything that happened to one stock row, and what was paid; a purchase can
 *  be removed. A dialog, as a second bottom sheet would dismiss the form. */
@Component({
  selector: 'app-history-dialog',
  templateUrl: './history-dialog.html',
  styleUrl: './history-dialog.scss',
  imports: [Dialog, ListState, MatButtonModule, MatIconModule, MatListModule],
})
export class HistoryDialog {
  private ref = inject(MatDialogRef<HistoryDialog, void>);
  private data = inject<HistoryDialogData>(MAT_DIALOG_DATA);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);

  readonly item = this.data.item;
  readonly entries = signal<ItemHistoryEntry[] | null>(null);
  readonly purchases = signal<Purchase[]>([]);
  readonly error = signal<string | null>(null);

  readonly loaded = computed(() => this.entries() !== null);
  readonly lines = computed(() => (this.entries() ?? []).map((e) => this.line(e)));

  /** Here as well as on the product page: a hand-typed item has no product. */
  readonly paid = computed(() =>
    this.purchases().map((p) => ({
      id: p.id,
      what: [
        formatMoney(p.amount_minor, p.currency),
        p.unit_price ? formatUnitPrice(p.unit_price, p.currency) : '',
      ]
        .filter((x) => x)
        .join(' · '),
      where: p.shop,
      when: ago(p.bought_at),
      cover: warrantyInfo(p.warranty_until),
    })),
  );

  constructor() {
    this.load();
  }

  load(): void {
    this.entries.set(null);
    this.error.set(null);
    this.api.itemHistory(this.item.id).subscribe({
      next: (h) => {
        this.entries.set(h.entries);
        this.purchases.set(h.purchases);
      },
      error: (e: unknown) => this.error.set(message(e)),
    });
  }

  private line(e: ItemHistoryEntry): Line {
    const shape = SHAPE[e.event];
    const how = e.quantity == null ? null : amount(e.quantity, this.item.unit);
    // Only a use records how much went; the rest record the level.
    const used = e.event === 'used';
    const detail = [
      !used && how ? `${how} on hand` : null,
      e.location ? (e.event === 'moved' ? `to ${e.location}` : e.location) : null,
    ]
      .filter((s) => s !== null)
      .join(' · ');
    return {
      id: e.id,
      icon: shape.icon,
      title: used && how ? `${shape.verb} ${how}` : shape.verb,
      detail,
      when: ago(e.at),
    };
  }

  removePurchase(id: number): void {
    this.api.deletePurchase(this.item.id, id).subscribe({
      next: () => {
        this.load();
        this.feedback.undo('Purchase removed', () => {
          this.api.restoreTrash('purchase', String(id)).subscribe({
            next: () => this.load(),
            error: (e: unknown) => this.feedback.error(`Could not undo${onlineHint(e)}`),
          });
        });
      },
      error: (e: unknown) => this.feedback.error(`Could not remove it${onlineHint(e)}`),
    });
  }

  close(): void {
    this.ref.close();
  }
}

function message(e: unknown): string {
  const failure = classifyApiError(e);
  switch (failure.kind) {
    case 'offline':
      // Not cached: a stale audit would deny a use that happened.
      return 'No connection — the history lives on the server.';
    case 'unauthenticated':
      return 'Signed out — sign in to see this.';
    case 'server':
      return 'Could not load the history.';
    default:
      return assertNever(failure);
  }
}
