import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { Router } from '@angular/router';

import { classifyApiError } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { fromLocalInput, toLocalInput } from '../../shared/local-time';
import { SheetHeader } from '../../shared/sheet-header';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';

/** The next whole hour — the default "when".
 *
 *  One rule rather than a guess at your habits: a trip planned now is for later
 *  today, and a field you can see is wrong is better than a clever default you
 *  have to check. */
function nextHour(from: Date): Date {
  const d = new Date(from);
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + 1);
  return d;
}

/** Plan a shop trip. The event goes only to Nextcloud Calendar
 *  (docs/design/overview.md §5); the confirmation names the calendar. The Buy
 *  list goes in the description from the screen, not the server, because the
 *  phone's local-first copy may be ahead of the sync. */
@Component({
  selector: 'app-trip-sheet',
  templateUrl: './trip-sheet.html',
  styleUrl: './trip-sheet.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    SheetHeader,
  ],
})
export class TripSheet {
  private ref = inject(MatBottomSheetRef<TripSheet>);
  private data = inject<{ shop?: string } | null>(MAT_BOTTOM_SHEET_DATA, { optional: true });
  private store = inject(ShoppingStore);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);
  private router = inject(Router);

  private allItems = toSignal(this.store.items$, { initialValue: [] as ShoppingDoc[] });

  readonly shop = signal(this.data?.shop ?? '');
  readonly when = signal(toLocalInput(nextHour(new Date())));
  readonly saving = signal(false);
  /** Set when the calendar isn't linked — the one failure a retry can't fix,
   *  so it is shown in the sheet with the way out rather than as a toast that
   *  disappears while you are still reading it. */
  readonly needsLinking = signal(false);

  /** What to bring home: the rows still to buy, in list order. A ticked-off row
   *  is already in the trolley of a trip that is happening now. */
  readonly items = computed(() => this.allItems().filter((i) => !i.done));

  readonly summary = computed(() => {
    const shop = this.shop().trim();
    return shop ? `Shop at ${shop}` : '';
  });

  readonly canSave = computed(
    () => !this.saving() && !!this.shop().trim() && fromLocalInput(this.when()) !== null,
  );

  save(): void {
    const shop = this.shop().trim();
    const startsAt = fromLocalInput(this.when());
    if (!shop || startsAt === null) return;
    this.saving.set(true);
    this.needsLinking.set(false);
    this.api
      .planShopTrip(
        shop,
        startsAt,
        this.items().map((i) => i.name),
      )
      .subscribe({
        next: (planned) => {
          this.saving.set(false);
          this.feedback.notify(`“${planned.summary}” added to ${planned.calendar}.`);
          this.ref.dismiss();
        },
        error: (e: unknown) => {
          this.saving.set(false);
          const failure = classifyApiError(e);
          // 409 is the backend saying the Nextcloud link is missing or lapsed.
          if (failure.kind === 'server' && failure.status === 409) {
            this.needsLinking.set(true);
            return;
          }
          this.feedback.error(
            failure.kind === 'offline'
              ? 'Planning a trip needs a connection — the calendar is Nextcloud’s.'
              : 'Couldn’t add it to your calendar.',
          );
        },
      });
  }

  openSettings(): void {
    this.ref.dismiss();
    void this.router.navigate(['/settings']);
  }

  close(): void {
    this.ref.dismiss();
  }
}
