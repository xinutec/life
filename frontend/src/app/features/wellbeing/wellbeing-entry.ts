import { Component, OnDestroy, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { Dialogs } from '@xinutec/ui-scaffold';

import { EMOTION_NODES, emotionColor, emotionDesc, emotionLabel } from '../../shared/emotion-wheel';
import { LifeApi } from '../../life-api';
import { Feedback } from '../../shared/feedback';
import { fromLocalInput, toLocalInput } from '../../shared/local-time';
import { SheetHeader } from '../../shared/sheet-header';
import {
  ENERGY_LEVELS,
  WELLBEING_SCORES,
  facesOf,
  isHalfStep,
  nextReading,
  scoreMeta,
  toPoints,
  toTenths,
} from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { EmotionPicker, EmotionPickerData } from './emotion-picker';

/** Edit one check-in. */
@Component({
  selector: 'app-wellbeing-entry',
  templateUrl: './wellbeing-entry.html',
  styleUrl: './wellbeing-entry.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    SheetHeader,
  ],
})
export class WellbeingEntry implements OnDestroy {
  private ref = inject(MatBottomSheetRef<WellbeingEntry>);
  private data = inject<{ ulid: string }>(MAT_BOTTOM_SHEET_DATA);
  private store = inject(WellbeingStore);
  private feedback = inject(Feedback);
  private dialog = inject(Dialogs);
  private api = inject(LifeApi);

  private deleting = false;
  // The model is warmed once per entry.
  private warmed = false;
  // Only a typed note is flushed on close, so a remote edit made meanwhile is
  // not overwritten by the stale original.
  private noteDirty = false;
  private items = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });

  readonly scores = WELLBEING_SCORES;
  readonly energies = ENERGY_LEVELS;
  readonly ulid = this.data.ulid;
  readonly entry = computed(() => this.items().find((e) => e.ulid === this.ulid));

  /** "okay–good · 3.5". */
  readonly scoreLabel = computed(() => {
    const tenths = this.entry()?.scoreTenths;
    if (tenths == null) return '';
    return `${scoreMeta(tenths).label} · ${toPoints(tenths)}/5`;
  });

  readonly note = signal(this.entry()?.note ?? '');
  readonly localTime = computed(() => {
    const e = this.entry();
    return e ? toLocalInput(e.recordedAt) : '';
  });

  ngOnDestroy(): void {
    if (this.deleting || !this.noteDirty) return;
    const e = this.entry();
    if (e && this.note().trim() !== (e.note ?? '')) this.saveNote();
  }

  /** Marks the note typed, for the flush on close. */
  onNoteInput(value: string): void {
    this.noteDirty = true;
    this.note.set(value);
    // First words: warm the model now, overlapping its ~60 s cold load with the
    // writing.
    if (!this.warmed && value.trim()) {
      this.warmed = true;
      this.api
        .warmEmotions({ candidates: EMOTION_NODES.map((n) => ({ token: n.token, desc: n.desc })) })
        .subscribe({ next: () => {}, error: () => {} });
    }
  }

  /** A tap next to the lit face makes the half-step; either face of a half-step
   *  collapses to it. */
  setScore(face: number): void {
    const now = this.entry()?.scoreTenths;
    void this.store.patch(this.ulid, { scoreTenths: nextReading(now, face) ?? toTenths(face) });
  }

  /** As `setScore`, but tapping the lone lit face clears energy. */
  setEnergy(face: number): void {
    const now = this.entry()?.energyTenths;
    if (now === toTenths(face)) {
      void this.store.patch(this.ulid, { energyTenths: null });
      return;
    }
    void this.store.patch(this.ulid, {
      energyTenths: now == null ? toTenths(face) : (nextReading(now, face) ?? toTenths(face)),
    });
  }

  isOn(reading: number | null | undefined, face: number): boolean {
    return reading != null && facesOf(reading).includes(face);
  }

  isHalf(reading: number | null | undefined, face: number): boolean {
    return reading != null && isHalfStep(reading) && facesOf(reading).includes(face);
  }

  emotionColor(token: string): string {
    return emotionColor(token);
  }

  emotionLabel(token: string): string {
    return emotionLabel(token);
  }

  emotionDesc(token: string): string {
    return emotionDesc(token);
  }

  /** Cancel leaves the emotions as they were. */
  editEmotions(): void {
    const ref = this.dialog.open<EmotionPicker, EmotionPickerData, string[] | undefined>(
      EmotionPicker,
      {
        data: { selected: [...(this.entry()?.emotions ?? [])], ulid: this.ulid, note: this.note() },
        panelClass: 'emotion-pane',
        width: '100%',
        maxWidth: '100vw',
        height: '100%',
        maxHeight: '100%',
        autoFocus: false,
      },
    );
    ref.afterClosed().subscribe((next: string[] | undefined) => {
      if (next) void this.store.patch(this.ulid, { emotions: next });
    });
  }

  removeEmotion(token: string): void {
    const next = (this.entry()?.emotions ?? []).filter((e) => e !== token);
    void this.store.patch(this.ulid, { emotions: next });
  }

  saveNote(): void {
    this.noteDirty = false;
    void this.store.patch(this.ulid, { note: this.note().trim() || null });
  }

  /** A half-typed or impossible time is ignored. */
  setTime(local: string): void {
    const recordedAt = fromLocalInput(local);
    if (recordedAt === null) return;
    void this.store.patch(this.ulid, { recordedAt });
  }

  remove(): void {
    const e = this.entry();
    this.deleting = true;
    void this.store.remove(this.ulid);
    this.ref.dismiss();
    if (e) this.feedback.undo('Check-in deleted', () => void this.store.undoDelete(e));
  }

  close(): void {
    this.ref.dismiss();
  }
}
