import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';
import { Sheets } from '@xinutec/ui-scaffold';

import { ExpiryInfo, expiryInfo } from '../../expiry';
import { amount } from '../../shared/amount';
import { LifeApi } from '../../life-api';
import { ProductThumb } from '../../product-thumb';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { ItemsStore, LocationsStore, locationPath } from '../../stores/catalog';
import { Item } from '../../models';
import { ItemSheet, ItemSheetData } from '../inventory/item-sheet';

type SortKey = 'name' | 'expiry';

/** Every item, filtered and sorted: the "find my stuff" screen. */
@Component({
  selector: 'app-items',
  templateUrl: './items.html',
  styleUrl: './items.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatButtonToggleModule,
    MatFormFieldModule,
    MatInputModule,
    MatListModule,
    MatIconModule,
    ProductThumb,
    ListState,
  ],
})
export class Items {
  private sheet = inject(Sheets);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);
  private itemsStore = inject(ItemsStore);
  private locationsStore = inject(LocationsStore);

  readonly items = computed(() => this.itemsStore.value() ?? []);
  readonly locations = computed(() => this.locationsStore.value() ?? []);
  readonly loaded = this.itemsStore.loaded;
  readonly loadError = this.itemsStore.error;
  readonly refreshing = this.itemsStore.refreshing;

  readonly query = signal('');
  readonly sort = signal<SortKey>('name');

  private readonly byId = computed(() => new Map(this.locations().map((l) => [l.id, l] as const)));

  private readonly locationOptions = computed(() =>
    this.locations().map((l) => ({ id: l.id, label: this.pathOf(l.id) })),
  );

  readonly visible = computed<Item[]>(() => {
    const q = this.query().trim().toLowerCase();
    const matches = q
      ? this.items().filter((it) =>
          [it.name, it.brand, this.location(it)].some((s) => s?.toLowerCase().includes(q)),
        )
      : this.items().slice();
    if (this.sort() === 'expiry') {
      // Undated items last.
      matches.sort((a, b) => (a.expiry ?? '9999').localeCompare(b.expiry ?? '9999'));
    } else {
      matches.sort((a, b) => a.name.localeCompare(b.name));
    }
    return matches;
  });
  readonly count = computed(() => this.visible().length);

  constructor() {
    this.reload();
    this.locationsStore.refresh();
  }

  reload(): void {
    this.itemsStore.refresh();
  }

  editItem(it: Item): void {
    const data: ItemSheetData = { item: it, locations: this.locationOptions() };
    this.sheet
      .open<ItemSheet, ItemSheetData, boolean>(ItemSheet, { data })
      .afterDismissed()
      .subscribe((saved) => {
        if (saved) this.reload();
      });
  }

  private pathOf(id: number | null): string {
    return locationPath(this.byId(), id);
  }

  deleteItem(id: number): void {
    this.api.deleteItem(id).subscribe({
      next: () => {
        this.reload();
        this.feedback.undo('Item deleted', () => {
          this.api.restoreTrash('item', String(id)).subscribe({
            next: () => this.reload(),
            error: () => this.feedback.error('Could not undo the delete'),
          });
        });
      },
      error: (e: unknown) => this.feedback.error(`Could not delete the item${onlineHint(e)}`),
    });
  }

  private location(it: Item): string {
    return this.pathOf(it.location_id).split(' › ').slice(-2).join(' › ');
  }

  /** "2 jar · food · Spice cupboard › Top shelf". */
  meta(it: Item): string {
    return [amount(it.quantity, it.unit), it.category, this.location(it)]
      .filter((s) => s)
      .join(' · ');
  }

  expiryOf(item: Item): ExpiryInfo {
    return expiryInfo(item.expiry ?? '', item.expiry_precision);
  }
}
