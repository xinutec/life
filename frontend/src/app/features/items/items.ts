import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';

import { ExpiryInfo, expiryInfo } from '../../expiry';
import { ProductThumb } from '../../product-thumb';
import { ListState } from '../../shared/list-state';
import { ItemsStore, LocationsStore } from '../../stores/catalog';
import { Item } from '../../models';
import { ItemSheet, ItemSheetData } from '../inventory/item-sheet';

type SortKey = 'name' | 'expiry';

/** The complete, flat list of every item that exists — display fields resolved
 *  through the catalog product (name/brand/image) where one is linked. The
 *  "find my stuff" surface: a live name/brand/location filter + a sort. Reached
 *  from the hamburger menu. */
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
  private sheet = inject(MatBottomSheet);
  private itemsStore = inject(ItemsStore);
  private locationsStore = inject(LocationsStore);

  // Views of the shared catalogs — retained across tabs and shared with
  // Inventory / Today (see CachedResource), so returning here is instant.
  readonly items = computed(() => this.itemsStore.value() ?? []);
  readonly locations = computed(() => this.locationsStore.value() ?? []);
  readonly loaded = this.itemsStore.loaded;
  readonly loadError = this.itemsStore.error;

  /** Live filter over name/brand/location, and the sort order. */
  readonly query = signal('');
  readonly sort = signal<SortKey>('name');

  private readonly byId = computed(() => new Map(this.locations().map((l) => [l.id, l] as const)));

  /** Full-path labels for the edit sheet's location dropdown. */
  private readonly locationOptions = computed(() =>
    this.locations().map((l) => ({ id: l.id, label: this.pathOf(l.id) })),
  );

  /** Items after the filter + sort — what the list renders. */
  readonly visible = computed<Item[]>(() => {
    const q = this.query().trim().toLowerCase();
    const matches = q
      ? this.items().filter((it) =>
          [it.name, it.brand, this.location(it)].some((s) => s?.toLowerCase().includes(q)),
        )
      : this.items().slice();
    if (this.sort() === 'expiry') {
      // Soonest expiry first; undated items sink to the bottom.
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

  /** Tap a row to edit it — the same add/edit bottom sheet Inventory uses,
   *  reused here so the search surface can fix an item in place. */
  editItem(it: Item): void {
    const data: ItemSheetData = { item: it, locations: this.locationOptions() };
    this.sheet
      .open<ItemSheet, ItemSheetData, boolean>(ItemSheet, { data })
      .afterDismissed()
      .subscribe((saved) => {
        if (saved) this.reload();
      });
  }

  /** Full location path (root → leaf), or '' when unplaced. */
  private pathOf(id: number | null): string {
    const map = this.byId();
    const names: string[] = [];
    const seen = new Set<number>();
    let cur = id;
    while (cur != null && !seen.has(cur)) {
      seen.add(cur);
      const loc = map.get(cur);
      if (!loc) break;
      names.unshift(loc.name);
      cur = loc.parent_id;
    }
    return names.join(' › ');
  }

  /** Last two segments of the location path, or '' when unplaced. */
  private location(it: Item): string {
    return this.pathOf(it.location_id).split(' › ').slice(-2).join(' › ');
  }

  /** Compact subtitle: "2 jar · food · Spice cupboard › Top shelf". */
  meta(it: Item): string {
    const qty = it.quantity == null ? '' : it.unit ? `${it.quantity} ${it.unit}` : `${it.quantity}`;
    return [qty, it.category, this.location(it)].filter((s) => s).join(' · ');
  }

  /** Urgency-aware expiry display (expired / soon / date). */
  expiryOf(expiry: string): ExpiryInfo {
    return expiryInfo(expiry);
  }
}
