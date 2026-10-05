import { Component, computed, inject } from '@angular/core';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { Sheets } from '@xinutec/ui-scaffold';

import { amount } from '../../shared/amount';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { ExpiryInfo, expiryInfo } from '../../expiry';
import { LifeApi } from '../../life-api';
import { ProductThumb } from '../../product-thumb';
import { ItemsStore, LocationsStore, locationPath } from '../../stores/catalog';
import { ShoppingStore } from '../../sync/shopping-store';
import { Item } from '../../models';
import { ItemSheet, ItemSheetData } from './item-sheet';
import { PlaceSheet, PlaceSheetData } from './place-sheet';
import { UseSheet, UseSheetData } from './use-sheet';

@Component({
  selector: 'app-inventory',
  templateUrl: './inventory.html',
  styleUrl: './inventory.scss',
  imports: [
    MatBottomSheetModule,
    MatListModule,
    MatIconModule,
    MatButtonModule,
    MatMenuModule,
    ProductThumb,
    ListState,
  ],
})
export class Inventory {
  private api = inject(LifeApi);
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  private itemsStore = inject(ItemsStore);
  private placesStore = inject(LocationsStore);
  private shopping = inject(ShoppingStore);

  private failed(what: string) {
    return (e: unknown) => {
      this.feedback.error(`Could not ${what}${onlineHint(e)}`);
    };
  }

  /** Deletes are tombstones; Undo restores from the trash. */
  private undoable(what: string, kind: 'item' | 'location', ref: number, reload: () => void) {
    this.feedback.undo(`${what} deleted`, () => {
      this.api.restoreTrash(kind, String(ref)).subscribe({
        next: () => reload(),
        error: this.failed('undo the delete'),
      });
    });
  }

  readonly items = computed(() => this.itemsStore.value() ?? []);
  readonly locations = computed(() => this.placesStore.value() ?? []);
  readonly itemsLoaded = this.itemsStore.loaded;
  readonly placesLoaded = this.placesStore.loaded;
  readonly itemsError = this.itemsStore.error;
  readonly placesError = this.placesStore.error;
  readonly itemsRefreshing = this.itemsStore.refreshing;
  readonly placesRefreshing = this.placesStore.refreshing;
  private byId = computed(() => new Map(this.locations().map((l) => [l.id, l] as const)));
  readonly locationOptions = computed(() =>
    this.locations().map((l) => ({ id: l.id, label: this.pathOf(l.id) })),
  );

  constructor() {
    this.reloadItems();
    this.reloadLocations();
  }

  addItem(): void {
    this.openItemSheet({ locations: this.locationOptions() });
  }

  editItem(it: Item): void {
    this.openItemSheet({ item: it, locations: this.locationOptions() });
  }

  private openItemSheet(data: ItemSheetData): void {
    this.sheet
      .open<ItemSheet, ItemSheetData, boolean>(ItemSheet, { data })
      .afterDismissed()
      .subscribe((saved) => {
        if (saved) this.reloadItems();
      });
  }

  /** "I used some of this", in the item's own unit. */
  useItem(it: Item): void {
    const data: UseSheetData = { item: it };
    this.sheet
      .open<UseSheet, UseSheetData, boolean>(UseSheet, { data })
      .afterDismissed()
      .subscribe((used) => {
        if (used) this.reloadItems();
      });
  }

  addPlace(): void {
    const data: PlaceSheetData = { locations: this.locationOptions() };
    this.sheet
      .open<PlaceSheet, PlaceSheetData, boolean>(PlaceSheet, { data })
      .afterDismissed()
      .subscribe((saved) => {
        if (saved) this.reloadLocations();
      });
  }

  reloadItems(): void {
    this.itemsStore.refresh();
  }
  reloadLocations(): void {
    this.placesStore.refresh();
  }

  pathOf(id: number | null): string {
    return locationPath(this.byId(), id);
  }

  qty(item: Item): string {
    return amount(item.quantity, item.unit);
  }

  expiryOf(item: Item): ExpiryInfo {
    return expiryInfo(item.expiry ?? '', item.expiry_precision);
  }

  /** The last two places of the path ("Spice cupboard › Top shelf"). */
  shortLoc(id: number | null): string {
    if (id == null) return '';
    return this.pathOf(id).split(' › ').slice(-2).join(' › ');
  }

  deletePlace(id: number): void {
    this.api.deleteLocation(id).subscribe({
      next: () => {
        this.reloadLocations();
        this.reloadItems(); // items there read as unplaced until restored
        this.undoable('Place', 'location', id, () => {
          this.reloadLocations();
          this.reloadItems();
        });
      },
      error: this.failed('delete the place'),
    });
  }

  /** Put this item on the Buy list unless it is already there. Not its
   *  quantity: what is owned is not what to buy. */
  async addToBuy(it: Item): Promise<void> {
    const identity = { name: it.name, barcode: it.barcode, product_id: it.product_id };
    if (await this.shopping.findActive(identity)) {
      this.feedback.notify(`${it.name} is already on the Buy list.`);
      return;
    }
    await this.shopping.add({
      name: it.name,
      quantity: null,
      unit: it.unit,
      barcode: it.barcode,
      category: it.category,
      product_id: it.product_id,
    });
    this.feedback.notify(`Added ${it.name} to the Buy list.`);
    // dev-lint: allow-ignored-error best-effort: the list add is what was asked for
    this.api.markLow(it.id).subscribe({ error: () => undefined });
  }

  deleteItem(id: number): void {
    this.api.deleteItem(id).subscribe({
      next: () => {
        this.reloadItems();
        this.undoable('Item', 'item', id, () => this.reloadItems());
      },
      error: this.failed('delete the item'),
    });
  }
}
