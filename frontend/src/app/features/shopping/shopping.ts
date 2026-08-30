import { Component, computed, effect, inject, signal } from "@angular/core";
import { toSignal } from "@angular/core/rxjs-interop";
import { MatBottomSheet, MatBottomSheetModule } from "@angular/material/bottom-sheet";
import { MatButtonModule } from "@angular/material/button";
import { MatCheckboxModule } from "@angular/material/checkbox";
import { MatIconModule } from "@angular/material/icon";
import { MatListModule } from "@angular/material/list";
import { MatMenuModule } from "@angular/material/menu";
import { Router } from "@angular/router";
import { catchError, forkJoin, map, of, tap } from "rxjs";

import { Feedback } from "../../shared/feedback";
import { isNotFound } from "../../shared/api-error";
import { ListState } from "../../shared/list-state";
import { LifeApi } from "../../life-api";
import { CoverageQuery, Source } from "../../models";
import { sourceLabel } from "../../shared/sources";
import { ProductThumb } from "../../product-thumb";
import { ShoppingDoc, ShoppingStore } from "../../sync/shopping-store";
import { BuyPrices, BuyRow, BuySheet } from "./buy-sheet";
import { ShoppingItemSheet } from "./shopping-item-sheet";
import { TripSheet } from "./trip-sheet";

@Component({
  selector: "app-shopping",
  templateUrl: "./shopping.html",
  styleUrl: "./shopping.scss",
  imports: [
    MatBottomSheetModule,
    MatListModule,
    MatIconModule,
    MatButtonModule,
    MatCheckboxModule,
    MatMenuModule,
    ProductThumb,
    ListState,
  ],
})
export class Shopping {
  private store = inject(ShoppingStore);
  private api = inject(LifeApi);
  private sheet = inject(MatBottomSheet);
  private feedback = inject(Feedback);
  private router = inject(Router);

  // Local-first: the list is the live RxDB query — instant, offline, reactive.
  readonly items = toSignal(this.store.items$, {
    initialValue: [] as ShoppingDoc[],
  });
  /** False until the local DB has produced its first result — so a cold start
   *  shows a spinner, not a flash of "nothing on the list". */
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), {
    initialValue: false,
  });
  readonly doneCount = computed(
    () => this.items().filter((i) => i.done).length,
  );
  readonly syncError = this.store.syncError;

  // --- Where can I get this trip? ---
  //
  // Answered from what earlier shop lookups already taught us, so it costs the
  // shops nothing and can run whenever the list changes. It says where a thing
  // is SOLD, never whether it is on the shelf tonight — the copy has to keep
  // that distinction, because a shopping list is exactly where a confident
  // wrong answer would send you to the wrong shop.

  private readonly coverage = signal<Map<string, Source[]>>(new Map());
  /** Whether the answer is missing because we couldn't ask, rather than because
   *  nothing is known. Shown, so an offline blank doesn't read as "nowhere". */
  private readonly coverageUnavailable = signal(false);

  constructor() {
    // Re-ask only when the rows worth asking about change — not on every store
    // emission (ticking one box would otherwise re-query the whole list).
    effect(() => {
      const rows = this.askable();
      if (!rows.length) {
        this.coverage.set(new Map());
        return;
      }
      this.api.shopCoverage(rows).subscribe({
        next: (answers) => {
          this.coverageUnavailable.set(false);
          this.coverage.set(new Map(answers.map((a) => [a.key, a.sources])));
        },
        // Enrichment, not the list itself: a failure leaves the Buy list working
        // and says the coverage line is unknown rather than empty.
        error: () => this.coverageUnavailable.set(true),
      });
    });
  }

  /** The un-done rows that carry something to look up. A ticked-off row is
   *  already in the trolley, and a free-text jotting has no identity to ask
   *  about. */
  private readonly askable = computed<CoverageQuery[]>(() =>
    this.items()
      .filter((it) => !it.done && (it.product_id != null || !!it.barcode?.trim()))
      .map((it) => ({ key: it.ulid, barcode: it.barcode, product_id: it.product_id })),
  );

  /** The shops known to sell this row, for its own line. */
  shopsFor(it: ShoppingDoc): Source[] {
    return this.coverage().get(it.ulid) ?? [];
  }

  /** "Asda · Waitrose" — the row's own shops, named as they are everywhere else. */
  shopLine(it: ShoppingDoc): string {
    return this.shopsFor(it).map(sourceLabel).join(" · ");
  }

  /** "Asda 6/8 · Waitrose 4/8", best first — the one-shop-trip question. Rows we
   *  can't ask about are counted separately rather than folded into the
   *  denominator, so a list of hand-typed jottings doesn't read as bad coverage. */
  readonly tripSummary = computed<{ shops: { label: string; have: number }[]; of: number; unknown: number } | null>(() => {
    const wanted = this.items().filter((it) => !it.done);
    if (!wanted.length || this.coverageUnavailable()) return null;
    const cover = this.coverage();
    const asked = wanted.filter((it) => cover.has(it.ulid));
    const counts = new Map<Source, number>();
    for (const it of asked) {
      for (const source of cover.get(it.ulid) ?? []) {
        counts.set(source, (counts.get(source) ?? 0) + 1);
      }
    }
    if (!counts.size) return null;
    return {
      shops: [...counts.entries()]
        .map(([source, have]) => ({ label: sourceLabel(source), have }))
        .sort((a, b) => b.have - a.have || a.label.localeCompare(b.label)),
      of: asked.length,
      unknown: wanted.length - asked.length,
    };
  });

  /** True when the coverage line is blank because we couldn't ask. */
  readonly coverageOffline = computed(
    () => this.coverageUnavailable() && this.askable().length > 0,
  );

  /** Whether there is a trip to plan. An empty list has nothing to go and get,
   *  and the button would be offering to schedule a walk. */
  readonly canPlanTrip = computed(() => this.items().some((i) => !i.done));

  /** Put the trip in the calendar. Pre-filled with the shop that covers most of
   *  the list — the coverage line right above this button is what the answer
   *  came from, so the sheet opens on the shop you just read about. */
  planTrip(): void {
    const best = this.tripSummary()?.shops[0]?.label;
    this.sheet.open(TripSheet, { data: { shop: best } });
  }

  /** The FAB's action: the add sheet (stays open for burst entry). */
  openAdd(): void {
    this.sheet.open(ShoppingItemSheet);
  }

  /** Open the item's edit sheet, pre-filled (from the row's ⋮ menu). */
  edit(it: ShoppingDoc): void {
    this.sheet.open(ShoppingItemSheet, { data: { ulid: it.ulid } });
  }

  /** Tap a row: open the product's detail page — the "bigger picture, all the
   *  info" view. A linked row goes straight there; a barcode-only row is looked
   *  up first; a row that's neither (a free-text jotting) has nothing to show,
   *  so it falls back to editing the entry. */
  view(it: ShoppingDoc): void {
    if (it.product_id != null) {
      void this.router.navigate(["/product", it.product_id]);
      return;
    }
    const barcode = it.barcode?.trim();
    if (!barcode) {
      this.edit(it);
      return;
    }
    this.api.lookupProduct(barcode).subscribe({
      next: (p) => void this.router.navigate(["/product", p.id]),
      error: (e: unknown) =>
        this.feedback.error(
          isNotFound(e)
            ? `No product found for ${barcode}.`
            : "Lookup failed — are you online?",
        ),
    });
  }

  toggle(it: ShoppingDoc): void {
    void this.store.setDone(it.ulid, !it.done);
  }

  remove(it: ShoppingDoc): void {
    void this.store.remove(it.ulid);
    this.undoableRemove([it]);
  }

  /** Offer Undo for removed rows. The store's two-layer undo (revive locally +
   *  server-side trash restore for synced rows) does the work per doc. */
  private undoableRemove(docs: ShoppingDoc[]): void {
    const what =
      docs.length === 1
        ? `Removed “${docs[0].name}”`
        : `Removed ${docs.length} items`;
    this.feedback.undo(what, () => {
      for (const doc of docs) void this.store.undoDelete(doc);
    });
  }

  /** Convert ticked-off rows into inventory items. Online-only (needs the
   *  inventory backend) and only for already-synced rows (those have a server
   *  id); the server soft-deletes them, which syncs back as a tombstone — we also
   *  remove locally for immediacy. Rows whose call fails STAY on the list (no
   *  silent local removal for something the server never inventoried), and the
   *  outcome is summarised either way. */
  buyDone(): void {
    const done = this.items().filter((i) => i.done && i.id != null);
    if (done.length === 0) return;
    const rows: BuyRow[] = done.map((it) => ({ id: it.id!, name: it.name }));
    this.sheet
      .open(BuySheet, { data: rows })
      .afterDismissed()
      .subscribe((res: BuyPrices | 'skip' | undefined) => {
        // Dismissed without choosing: buy nothing. Closing a sheet you opened by
        // mistake must not empty the list.
        if (res === undefined) return;
        this.completeBuy(done, res === 'skip' ? null : res);
      });
  }

  /** The buy itself, once it is known whether prices were recorded. */
  private completeBuy(done: ShoppingDoc[], priced: BuyPrices | null): void {
    const buys = done.map((it) => {
      const minor = priced?.prices.get(it.id!);
      const purchase =
        priced && minor !== undefined ? { shop: priced.shop, amount_minor: minor } : undefined;
      return this.api.buyShopping(it.id!, purchase).pipe(
        tap(() => void this.store.remove(it.ulid)), // remove as each one lands
        map(() => true),
        catchError(() => of(false)),
      );
    });
    forkJoin(buys).subscribe((flags) => {
      const ok = flags.filter(Boolean).length;
      const failed = flags.length - ok;
      if (failed > 0) {
        this.feedback.error(
          `${ok} added to inventory; ${failed} failed and stayed on the list.`,
        );
      } else {
        this.feedback.notify(
          ok === 1 ? "Added to inventory." : `${ok} added to inventory.`,
        );
      }
    });
  }

  clearDone(): void {
    const cleared = this.items().filter((i) => i.done);
    void this.store.clearDone();
    if (cleared.length > 0) this.undoableRemove(cleared);
  }

  label(it: ShoppingDoc): string {
    if (it.quantity == null) return "";
    return it.unit ? `${it.quantity} ${it.unit}` : `${it.quantity}`;
  }
}
