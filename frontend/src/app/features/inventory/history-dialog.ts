import { Component, computed, inject, signal } from '@angular/core';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';

import { ago } from '../../shared/ago';
import { amount } from '../../shared/amount';
import { fromMinorUnits } from '../../shared/money';
import { assertNever, classifyApiError } from '../../shared/api-error';
import { Dialog } from '../../shared/dialog';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { Item, ItemEvent, ItemHistoryEntry, Purchase } from '../../models';

export interface HistoryDialogData {
  item: Item;
}

/** One history line as the list renders it. */
interface Line {
  id: number;
  icon: string;
  /** The verb, with the amount folded in when the amount IS the verb's object
   *  ("Used 200 g"). */
  title: string;
  /** Where, and how much was on hand — the parts that are context rather than
   *  the event itself. Empty when there is nothing to say. */
  detail: string;
  when: string;
}

/** What each event is called and what it looks like. Keyed on the closed
 *  `ItemEvent` union, so a new event kind fails the build here rather than
 *  rendering as a blank row in a list whose whole job is to account for what
 *  happened. */
const SHAPE: Record<ItemEvent, { icon: string; verb: string }> = {
  added: { icon: 'add_circle_outline', verb: 'Added' },
  moved: { icon: 'swap_horiz', verb: 'Moved' },
  used: { icon: 'remove_circle_outline', verb: 'Used' },
  removed: { icon: 'delete_outline', verb: 'Deleted' },
  restored: { icon: 'undo', verb: 'Restored' },
};

/** Everything that has happened to one stock row.
 *
 *  A dialog rather than a bottom sheet on purpose: it opens from the item
 *  sheet, and `MatBottomSheet` holds one sheet at a time — a second would
 *  dismiss the edit form under it and take any unsaved typing with it. The
 *  product picker opens from the same place for the same reason.
 *
 *  Read-only, and it stays that way. The audit is append-only; an item's own
 *  numbers are edited in the sheet that opened this. */
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

  readonly item = this.data.item;
  readonly entries = signal<ItemHistoryEntry[] | null>(null);
  readonly purchases = signal<Purchase[]>([]);
  readonly error = signal<string | null>(null);

  readonly loaded = computed(() => this.entries() !== null);
  readonly lines = computed(() => (this.entries() ?? []).map((e) => this.line(e)));

  /** What this row cost, newest first.
   *
   *  Shown HERE and not only on the product page, because for a hand-typed
   *  buy-list row the product page cannot show it: there is no barcode and no
   *  catalogue product to hang it on, and the item is the only key that always
   *  exists. Without this the price was recorded and then unreachable. */
  readonly paid = computed(() =>
    this.purchases().map((p) => ({
      id: p.id,
      // The rate leads when there is one — it is the comparable number.
      what: [
        `${p.currency === 'GBP' ? '£' : p.currency + ' '}${fromMinorUnits(p.amount_minor)}`,
        p.unit_amount_minor != null && p.unit_measure
          ? `£${fromMinorUnits(p.unit_amount_minor)}/${p.unit_measure}`
          : '',
      ]
        .filter((x) => x)
        .join(' · '),
      where: p.shop,
      when: ago(new Date(p.bought_at).getTime()),
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
    // In the item's own unit — the audit stores a bare number, because a use is
    // only ever recorded in the unit the row measures itself in (the server
    // refuses a mismatch rather than converting).
    const how = e.quantity == null ? null : amount(e.quantity, this.item.unit);
    // A use is the ONLY event whose quantity is a delta — how much went. Every
    // other one records the level at the time. Saying so in words is the whole
    // difference between "you used 200g" and "it held 200g", which the number
    // alone cannot carry.
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

  close(): void {
    this.ref.close();
  }
}

function message(e: unknown): string {
  const failure = classifyApiError(e);
  switch (failure.kind) {
    case 'offline':
      // Not cached by the service worker and deliberately not: an audit read
      // from a stale cache would say a use had not happened when it had.
      return 'No connection — the history lives on the server.';
    case 'unauthenticated':
      return 'Signed out — sign in to see this.';
    case 'server':
      return 'Could not load the history.';
    default:
      return assertNever(failure);
  }
}
