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
  EmotionLeafDef,
  emotionColor,
  emotionDesc,
  emotionLabel,
  emotionToken,
  searchEmotions,
} from '../../shared/emotion-wheel';

export interface EmotionPickerData {
  /** The emotions already on the entry (qualified tokens, or legacy bare words). */
  selected: string[];
}

/** How long a press must be held before it counts as a "peek" (show the gloss)
 *  rather than a tap (select). */
const HOLD_MS = 350;

/** The feeling being peeked at (held) — its word, gloss, and family colour. */
interface Peek {
  name: string;
  desc: string;
  color: string;
}

/** Browse and search the feelings wheel to add/remove emotions on a check-in.
 *  Mobile-first: a search box that flattens the whole vocabulary, plus a
 *  colour-coded drill-down accordion to explore it. Multi-select and
 *  independent — any number of feelings from any families. Works throughout in
 *  qualified `Core/Leaf` tokens (so a leaf that appears under two cores stays two
 *  distinct choices); the incoming selection is normalised to tokens on open.
 *  Closes with the new set of tokens (Done), or `undefined` if dismissed. */
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
  // The working set holds canonical tokens; legacy bare words are upgraded here,
  // the single normalisation boundary — every other method deals only in tokens.
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected.map(emotionToken)));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  /** The qualified token for a leaf under a given core. */
  tokenOf(core: EmotionCore, leaf: string): string {
    return `${core.name}/${leaf}`;
  }

  isSelected(token: string): boolean {
    return this.selected().has(token);
  }

  color(token: string): string {
    return emotionColor(token);
  }

  /** The bare leaf word for a token, for chip display. */
  label(token: string): string {
    return emotionLabel(token);
  }

  /** The brief gloss for a token (tooltip on the selected chip). */
  desc(token: string): string {
    return emotionDesc(token);
  }

  /** How many selected tokens fall under a given core (for the panel badge). */
  coreCount(core: EmotionCore): number {
    const sel = this.selected();
    let n = 0;
    for (const g of core.groups)
      for (const leaf of g.leaves) if (sel.has(this.tokenOf(core, leaf.name))) n++;
    return n;
  }

  toggle(token: string): void {
    const next = new Set(this.selected());
    if (!next.delete(token)) next.add(token);
    this.selected.set(next);
  }

  // Press-and-hold to peek a leaf's gloss without selecting it. A quick tap
  // selects (the common case, kept light); holding shows the description while
  // the finger is down and suppresses the select that would otherwise follow.
  // Browse stays compact — the gloss is one hold away, and always inline in
  // search — so a leaf's meaning is never gesture-only.
  private holdTimer: ReturnType<typeof setTimeout> | null = null;
  private longPressed = false;
  readonly peeked = signal<Peek | null>(null);

  pressStart(core: EmotionCore, leaf: EmotionLeafDef): void {
    this.longPressed = false;
    this.clearHold();
    this.holdTimer = setTimeout(() => {
      this.longPressed = true;
      this.peeked.set({ name: leaf.name, desc: leaf.desc, color: core.color });
    }, HOLD_MS);
  }

  /** Movement before the hold fires means the finger is scrolling, not peeking. */
  pressMove(): void {
    if (!this.longPressed) this.clearHold();
  }

  pressEnd(): void {
    this.clearHold();
    this.peeked.set(null);
  }

  private clearHold(): void {
    if (this.holdTimer !== null) {
      clearTimeout(this.holdTimer);
      this.holdTimer = null;
    }
  }

  /** Chip tap: select — unless it was the release of a peek hold, which selects
   *  nothing (the whole point of the hold). */
  choose(token: string): void {
    if (this.longPressed) {
      this.longPressed = false;
      return;
    }
    this.toggle(token);
  }

  done(): void {
    this.ref.close([...this.selected()]);
  }

  cancel(): void {
    this.ref.close(undefined);
  }
}
