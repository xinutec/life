import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';

import { Dialog } from '../../shared/dialog';
import { ShopCandidate, ShopProvider, Shops } from '../../shop';

export interface ShopSearchData {
  provider: ShopProvider;
  /** Prefill (usually the item's current name); searched immediately if set. */
  initialQuery: string;
}

/** Search a shop by name and pick a product. Runs a hidden WebView on the shop
 *  site (via [[Shops]]) so it's only reachable inside the Android app. Closes
 *  with the chosen [[ShopCandidate]], or `null`/undefined if cancelled. The
 *  caller fetches full detail + imports — this dialog only picks. */
@Component({
  selector: 'app-shop-search-dialog',
  templateUrl: './shop-search-dialog.html',
  styleUrl: './shop-search-dialog.scss',
  imports: [
    Dialog,
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatListModule,
    MatProgressSpinnerModule,
  ],
})
export class ShopSearchDialog {
  private ref = inject<MatDialogRef<ShopSearchDialog, ShopCandidate | null>>(MatDialogRef);
  private data = inject<ShopSearchData>(MAT_DIALOG_DATA);
  private shops = inject(Shops);

  readonly provider = this.data.provider;
  readonly query = signal(this.data.initialQuery.trim());
  readonly searching = signal(false);
  readonly searched = signal(false);
  readonly candidates = signal<ShopCandidate[]>([]);
  readonly error = signal<string | null>(null);

  constructor() {
    if (this.query()) this.run();
  }

  run(): void {
    const q = this.query().trim();
    if (!q || this.searching()) return;
    this.searching.set(true);
    this.error.set(null);
    this.shops
      .search(this.provider, q)
      .then((cands) => {
        this.candidates.set(cands);
        this.searched.set(true);
      })
      .catch((e: unknown) => this.error.set(this.message(e)))
      .finally(() => this.searching.set(false));
  }

  pick(c: ShopCandidate): void {
    this.ref.close(c);
  }

  close(): void {
    this.ref.close(null);
  }

  /** A search that finds nothing usually means "not signed in" (the shop only
   *  returns results to a logged-in session), so nudge toward Connect. */
  private message(e: unknown): string {
    const raw = e instanceof Error ? e.message : String(e);
    return /only available in the app/i.test(raw)
      ? raw
      : `Search failed — try “Connect ${this.provider.displayName}” first.`;
  }
}
