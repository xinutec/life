import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { Sheets } from '@xinutec/ui-scaffold';

import { classifyApiError } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { fromLocalInput, toLocalInput } from '../../shared/local-time';
import { SheetHeader } from '../../shared/sheet-header';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';

/** The next whole hour: the default "when". */
function nextHour(from: Date): Date {
  const d = new Date(from);
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + 1);
  return d;
}

/** Plan a shop trip in the Nextcloud calendar. The list comes from this device,
 *  which may be ahead of the sync. */
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
  private sheets = inject(Sheets);
  private data = inject<{ shop?: string } | null>(MAT_BOTTOM_SHEET_DATA, { optional: true });
  private store = inject(ShoppingStore);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);

  private allItems = toSignal(this.store.items$, { initialValue: [] as ShoppingDoc[] });

  readonly shop = signal(this.data?.shop ?? '');
  readonly when = signal(toLocalInput(nextHour(new Date())));
  readonly saving = signal(false);
  /** The calendar is not linked: shown in the sheet, with the way out. */
  readonly needsLinking = signal(false);

  /** The rows still to buy. */
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
    void this.sheets.dismissTo(this.ref, ['/settings']);
  }

  close(): void {
    this.ref.dismiss();
  }
}
