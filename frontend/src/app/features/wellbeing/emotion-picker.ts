import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatExpansionModule } from '@angular/material/expansion';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import {
  EMOTION_WHEEL,
  EmotionCore,
  emotionColor,
  searchEmotions,
} from '../../shared/emotion-wheel';

export interface EmotionPickerData {
  /** The leaf words already on the entry. */
  selected: string[];
}

/** Browse and search the feelings wheel to add/remove emotions on a check-in.
 *  Mobile-first: a search box that flattens the whole vocabulary, plus a
 *  colour-coded drill-down accordion to explore it. Multi-select and
 *  independent — any number of feelings from any families. Closes with the new
 *  set of leaf words (Done), or `undefined` if dismissed (changes discarded). */
@Component({
  selector: 'app-emotion-picker',
  templateUrl: './emotion-picker.html',
  styleUrl: './emotion-picker.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatExpansionModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
  ],
})
export class EmotionPicker {
  private ref = inject<MatDialogRef<EmotionPicker, string[] | undefined>>(MatDialogRef);
  private data = inject<EmotionPickerData>(MAT_DIALOG_DATA);

  readonly wheel = EMOTION_WHEEL;
  readonly query = signal('');
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  isSelected(leaf: string): boolean {
    return this.selected().has(leaf);
  }

  color(leaf: string): string {
    return emotionColor(leaf);
  }

  /** How many selected leaves fall under a given core (for the panel badge). */
  coreCount(core: EmotionCore): number {
    const sel = this.selected();
    let n = 0;
    for (const g of core.groups) for (const leaf of g.leaves) if (sel.has(leaf)) n++;
    return n;
  }

  toggle(leaf: string): void {
    const next = new Set(this.selected());
    if (!next.delete(leaf)) next.add(leaf);
    this.selected.set(next);
  }

  done(): void {
    this.ref.close([...this.selected()]);
  }

  cancel(): void {
    this.ref.close(undefined);
  }
}
