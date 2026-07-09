import { Component, OnDestroy, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import {
  MAT_BOTTOM_SHEET_DATA,
  MatBottomSheetRef,
} from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { emotionColor } from '../../shared/emotion-wheel';
import { Feedback } from '../../shared/feedback';
import { SheetHeader } from '../../shared/sheet-header';
import { ENERGY_LEVELS, WELLBEING_SCORES } from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { EmotionPicker } from './emotion-picker';

/** ISO instant → the value a <input type="datetime-local"> expects (local). */
function toLocalInput(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Edit one check-in: change the score, add/edit a note, adjust the time (to
 *  backdate "this morning"), or delete it. */
@Component({
  selector: 'app-wellbeing-entry',
  templateUrl: './wellbeing-entry.html',
  styleUrl: './wellbeing-entry.scss',
  imports: [FormsModule, MatButtonModule, MatFormFieldModule, MatIconModule, MatInputModule, SheetHeader],
})
export class WellbeingEntry implements OnDestroy {
  private ref = inject(MatBottomSheetRef<WellbeingEntry>);
  private data = inject<{ ulid: string }>(MAT_BOTTOM_SHEET_DATA);
  private store = inject(WellbeingStore);
  private feedback = inject(Feedback);
  private dialog = inject(MatDialog);

  private deleting = false;
  // True once the user has actually typed in the note field. Guards the
  // dismiss-time flush so it never writes back the value the sheet opened with —
  // otherwise an edit that landed remotely while the sheet was open would be
  // clobbered by the stale original on close.
  private noteDirty = false;
  private items = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });

  readonly scores = WELLBEING_SCORES;
  readonly energies = ENERGY_LEVELS;
  readonly ulid = this.data.ulid;
  readonly entry = computed(() => this.items().find((e) => e.ulid === this.ulid));

  readonly note = signal(this.entry()?.note ?? '');
  readonly localTime = computed(() => {
    const e = this.entry();
    return e ? toLocalInput(e.recordedAt) : '';
  });

  // Flush an in-progress note edit if the sheet is dismissed without a blur —
  // but only one the user actually typed (noteDirty), never the seeded original.
  ngOnDestroy(): void {
    if (this.deleting || !this.noteDirty) return;
    const e = this.entry();
    if (e && this.note().trim() !== (e.note ?? '')) this.saveNote();
  }

  /** The note textarea's input handler: record the edit and mark it dirty so a
   *  dismiss will flush it (a bare `note.set` would not). */
  onNoteInput(value: string): void {
    this.noteDirty = true;
    this.note.set(value);
  }

  setScore(score: number): void {
    void this.store.patch(this.ulid, { score });
  }

  /** Toggle the energy reading — tapping the active level clears it to null,
   *  keeping it optional (a mood-only check-in). */
  setEnergy(energy: number): void {
    const next = this.entry()?.energy === energy ? null : energy;
    void this.store.patch(this.ulid, { energy: next });
  }

  emotionColor(leaf: string): string {
    return emotionColor(leaf);
  }

  /** Open the feelings-wheel picker seeded with the current set; on Done, store
   *  the new selection (Cancel returns undefined and leaves it untouched). */
  editEmotions(): void {
    const ref = this.dialog.open(EmotionPicker, {
      data: { selected: [...(this.entry()?.emotions ?? [])] },
      panelClass: 'emotion-pane',
      width: '100%',
      maxWidth: '100vw',
      height: '100%',
      maxHeight: '100%',
      autoFocus: false,
    });
    ref.afterClosed().subscribe((next: string[] | undefined) => {
      if (next) void this.store.patch(this.ulid, { emotions: next });
    });
  }

  removeEmotion(leaf: string): void {
    const next = (this.entry()?.emotions ?? []).filter((e) => e !== leaf);
    void this.store.patch(this.ulid, { emotions: next });
  }

  saveNote(): void {
    this.noteDirty = false;
    void this.store.patch(this.ulid, { note: this.note().trim() || null });
  }

  setTime(local: string): void {
    if (!local) return;
    void this.store.patch(this.ulid, { recordedAt: new Date(local).toISOString() });
  }

  remove(): void {
    const e = this.entry();
    this.deleting = true;
    void this.store.remove(this.ulid);
    this.ref.dismiss();
    // Two-layer undo: local revive + server-side trash restore for synced rows.
    // A plain local revive can't survive the server's set-only tombstone.
    if (e) this.feedback.undo('Check-in deleted', () => void this.store.undoDelete(e));
  }

  close(): void {
    this.ref.dismiss();
  }
}
