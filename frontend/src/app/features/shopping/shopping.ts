import { Component, computed, inject, signal } from '@angular/core';
import { takeUntilDestroyed, toObservable, toSignal } from '@angular/core/rxjs-interop';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { Router } from '@angular/router';
import { Sheets } from '@xinutec/ui-scaffold';
import { catchError, forkJoin, map, of, switchMap, tap } from 'rxjs';

import { amount } from '../../shared/amount';
import { Feedback } from '../../shared/feedback';
import { isNotFound } from '../../shared/api-error';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { CoverageQuery, RowPrice, Source } from '../../models';
import { formatMoney } from '../../shared/money';
import { sourceLabel } from '../../shared/sources';
import { ProductThumb } from '../../product-thumb';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { BuyPrices, BuyRow, BuySheet } from './buy-sheet';
import { ShoppingItemSheet } from './shopping-item-sheet';
import { TripSheet } from './trip-sheet';

@Component({
  selector: 'app-shopping',
  templateUrl: './shopping.html',
  styleUrl: './shopping.scss',
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
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  private router = inject(Router);

  readonly items = toSignal(this.store.items$, {
    initialValue: [] as ShoppingDoc[],
  });
  /** False until the local DB has produced its first result. */
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), {
    initialValue: false,
  });
  readonly doneCount = computed(() => this.items().filter((i) => i.done).length);
  readonly syncError = this.store.syncError;

  // Where things are SOLD, from earlier lookups: never whether they are on the
  // shelf, and it costs the shops nothing.

  private readonly coverage = signal<Map<string, Source[]>>(new Map());
  private readonly prices = signal<Map<string, RowPrice[]>>(new Map());
  /** Unknown because we could not ask, which must not read as "nowhere". */
  private readonly coverageUnavailable = signal(false);

  /** The un-done rows with an identity to look up. Equal by content, so an edit
   *  that changes none of it asks nothing. */
  private readonly askable = computed<CoverageQuery[]>(
    () =>
      this.items()
        .filter((it) => !it.done && (it.product_id != null || !!it.barcode?.trim()))
        .map((it) => ({ key: it.ulid, barcode: it.barcode, product_id: it.product_id })),
    { equal: sameQueries },
  );

  constructor() {
    // switchMap: a slow older answer can never overwrite a newer one.
    toObservable(this.askable)
      .pipe(
        switchMap((rows) =>
          rows.length
            ? this.api.shopCoverage(rows).pipe(
                map((answers) => ({ ok: true as const, answers })),
                catchError(() => of({ ok: false as const })),
              )
            : of({ ok: true as const, answers: [] }),
        ),
        takeUntilDestroyed(),
      )
      .subscribe((r) => {
        this.coverageUnavailable.set(!r.ok);
        if (!r.ok) return;
        this.coverage.set(new Map(r.answers.map((a) => [a.key, a.sources])));
        this.prices.set(new Map(r.answers.map((a) => [a.key, a.prices])));
      });
  }

  shopsFor(it: ShoppingDoc): Source[] {
    return this.coverage().get(it.ulid) ?? [];
  }

  shopLine(it: ShoppingDoc): string {
    return this.shopsFor(it).map(sourceLabel).join(' · ');
  }

  /** "Asda 6/8 · Waitrose 4/8". Rows we cannot ask about are counted apart, not
   *  as bad coverage. */
  readonly tripSummary = computed<{
    shops: { label: string; have: number }[];
    of: number;
    unknown: number;
  } | null>(() => {
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

  /** Each shop's shelf prices over the rows it has priced; the counts say the
   *  totals do not compare. GBP only. */
  readonly estimates = computed<{ label: string; total: string; priced: number; of: number }[]>(
    () => {
      const wanted = this.items().filter((it) => !it.done);
      const byRow = this.prices();
      const totals = new Map<Source, { minor: number; priced: number }>();
      for (const it of wanted) {
        for (const p of byRow.get(it.ulid) ?? []) {
          if (p.currency !== 'GBP') continue;
          const t = totals.get(p.source) ?? { minor: 0, priced: 0 };
          t.minor += p.amount_minor * packsOf(it);
          t.priced += 1;
          totals.set(p.source, t);
        }
      }
      return [...totals.entries()]
        .map(([source, t]) => ({
          label: sourceLabel(source),
          total: formatMoney(t.minor, 'GBP'),
          priced: t.priced,
          of: wanted.length,
        }))
        .sort((a, b) => b.priced - a.priced || a.label.localeCompare(b.label));
    },
  );

  readonly coverageOffline = computed(
    () => this.coverageUnavailable() && this.askable().length > 0,
  );

  readonly canPlanTrip = computed(() => this.items().some((i) => !i.done));

  /** Pre-filled with the shop covering most of the list. */
  planTrip(): void {
    const best = this.tripSummary()?.shops[0]?.label;
    this.sheet.open(TripSheet, { data: { shop: best } });
  }

  openAdd(): void {
    this.sheet.open(ShoppingItemSheet);
  }

  edit(it: ShoppingDoc): void {
    this.sheet.open(ShoppingItemSheet, { data: { ulid: it.ulid } });
  }

  /** Open the row's product, looking a barcode up first; a free-text row opens
   *  its edit sheet instead. */
  view(it: ShoppingDoc): void {
    if (it.product_id != null) {
      void this.router.navigate(['/product', it.product_id]);
      return;
    }
    const barcode = it.barcode?.trim();
    if (!barcode) {
      this.edit(it);
      return;
    }
    this.api.lookupProduct(barcode).subscribe({
      next: (p) => void this.router.navigate(['/product', p.id]),
      error: (e: unknown) =>
        this.feedback.error(
          isNotFound(e) ? `No product found for ${barcode}.` : 'Lookup failed — are you online?',
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

  private undoableRemove(docs: ShoppingDoc[]): void {
    const what = docs.length === 1 ? `Removed “${docs[0].name}”` : `Removed ${docs.length} items`;
    this.feedback.undo(what, () => {
      for (const doc of docs) void this.store.undoDelete(doc);
    });
  }

  /** Turn synced ticked rows into inventory items. A row whose call fails stays
   *  on the list. */
  buyDone(): void {
    const ticked = this.items().filter((i) => i.done);
    const done = ticked.filter((i) => i.id != null);
    // Said, not skipped: the button counted them.
    const unsynced = notSyncedYet(ticked.length - done.length);
    if (done.length === 0) {
      if (unsynced) this.feedback.notify(unsynced);
      return;
    }
    const rows: BuyRow[] = done.map((it) => ({ id: it.id!, name: it.name }));
    this.sheet
      .open<BuySheet, BuyRow[], BuyPrices | 'skip'>(BuySheet, { data: rows })
      .afterDismissed()
      .subscribe((res: BuyPrices | 'skip' | undefined) => {
        // Dismissed: buy nothing.
        if (res === undefined) return;
        this.completeBuy(done, res === 'skip' ? null : res, unsynced);
      });
  }

  private completeBuy(done: ShoppingDoc[], priced: BuyPrices | null, unsynced: string): void {
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
      const tail = unsynced ? ` ${unsynced}` : '';
      if (failed > 0) {
        this.feedback.error(
          `${ok} added to inventory; ${failed} failed and stayed on the list.${tail}`,
        );
      } else {
        this.feedback.notify(
          `${ok === 1 ? 'Added to inventory.' : `${ok} added to inventory.`}${tail}`,
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
    return amount(it.quantity, it.unit);
  }
}

const MEASURES = /^(m?g|kg|grams?|kilos?|ml|cl|l|litres?|liters?|oz|lbs?)$/i;

/** Packs a row asks for: a count multiplies the pack price, a measure ("500 g")
 *  is one pack. */
function packsOf(it: ShoppingDoc): number {
  const q = it.quantity;
  const counted = !MEASURES.test(it.unit?.trim() ?? '');
  return counted && q != null && Number.isInteger(q) && q > 0 ? q : 1;
}

function sameQueries(a: readonly CoverageQuery[], b: readonly CoverageQuery[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (q, i) =>
        q.key === b[i].key && q.barcode === b[i].barcode && q.product_id === b[i].product_id,
    )
  );
}

function notSyncedYet(n: number): string {
  if (n === 0) return '';
  return `${n} not synced yet, so ${n === 1 ? 'it stays' : 'they stay'} ticked.`;
}
