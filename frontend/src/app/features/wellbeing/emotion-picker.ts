import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { LifeApi } from '../../life-api';
import {
  EMOTION_NODES,
  EMOTION_WHEEL,
  EmotionCore,
  EmotionNode,
  emotionColor,
  emotionDesc,
  emotionLabel,
  emotionNode,
  emotionToken,
  searchEmotions,
} from '../../shared/emotion-wheel';

export interface EmotionPickerData {
  /** The emotions already on the entry (qualified tokens, or legacy bare words). */
  selected: string[];
  /** The check-in note, if any — used to fetch Claude's suggested feelings. */
  note?: string;
}

/** Browse and search the feelings wheel to add/remove emotions on a check-in.
 *
 *  Browse is a mosaic, not a grid of controls: each family is a region of its own
 *  colour, and every word in it is plain text — colour carries the family, weight
 *  the ring. Chrome appears only on the words you actually chose. The whole
 *  vocabulary is on one surface, because the point of holding ~130 words is to
 *  offer you one you would not have thought to search for; hiding them behind
 *  drill-downs or a recents list would quietly return you to the same dozen.
 *
 *  Meaning is per-word and opens in place: a small ⓘ beside a word drops its
 *  gloss directly underneath it. One at a time, no overlay, no gesture.
 *
 *  Works throughout in qualified `Core/Name` tokens (so a word that appears under
 *  two cores stays two distinct choices, and a group stays distinct from its
 *  leaves); the incoming selection is normalised to tokens on open. Closes with
 *  the new set of tokens (Done), or `undefined` if dismissed. */
@Component({
  selector: 'app-emotion-picker',
  templateUrl: './emotion-picker.html',
  styleUrl: './emotion-picker.scss',
  imports: [
    FormsModule,
    NgTemplateOutlet,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
  ],
})
export class EmotionPicker {
  private ref = inject<MatDialogRef<EmotionPicker, string[] | undefined>>(MatDialogRef);
  private data = inject<EmotionPickerData>(MAT_DIALOG_DATA);
  private api = inject(LifeApi);

  readonly wheel = EMOTION_WHEEL;
  readonly query = signal('');
  // The working set holds canonical tokens; legacy bare words are upgraded here,
  // the single normalisation boundary — every other method deals only in tokens.
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected.map(emotionToken)));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  // Claude's suggestions from the note, shown at the head of the mosaic. Pure
  // enhancement: they arrive a beat after the picker opens, and if the note is
  // empty or the call fails (offline, no API key) the picker is unchanged.
  readonly suggesting = signal(false);
  readonly suggestions = signal<readonly EmotionNode[]>([]);

  constructor() {
    const note = (this.data.note ?? '').trim();
    if (!note) return;
    // The candidates are the whole wheel — sending them keeps the server's ranking
    // in lockstep with exactly what the user can pick, with no second copy to drift.
    const candidates = EMOTION_NODES.map((n) => ({ token: n.token, desc: n.desc }));
    const already = [...this.selected()];
    this.suggesting.set(true);
    this.api
      .suggestEmotions({ note, candidates, already })
      .pipe(takeUntilDestroyed())
      .subscribe({
        next: (r) => {
          this.suggestions.set(
            (r?.suggestions ?? []).map(emotionNode).filter((n): n is EmotionNode => !!n),
          );
          this.suggesting.set(false);
        },
        error: () => this.suggesting.set(false),
      });
  }

  /** The qualified token for a node — group or leaf — under a given core. */
  tokenOf(core: EmotionCore, name: string): string {
    return `${core.name}/${name}`;
  }

  /** Where a search hit sits: a group shows just its family, a leaf its group too. */
  path(node: EmotionNode): string {
    return node.kind === 'group' ? node.core : `${node.core} › ${node.secondary}`;
  }

  isSelected(token: string): boolean {
    return this.selected().has(token);
  }

  color(token: string): string {
    return emotionColor(token);
  }

  /** The bare word for a token, for chip display. */
  label(token: string): string {
    return emotionLabel(token);
  }

  /** The brief gloss for a token (tooltip on the selected chip). */
  desc(token: string): string {
    return emotionDesc(token);
  }

  /** Every selectable word in a family — both rings — for the family's count. */
  wordCount(core: EmotionCore): number {
    return core.groups.reduce((n, g) => n + g.leaves.length + 1, 0);
  }

  /** How many selected tokens fall under a given core (for the family badge) —
   *  groups count too, since a group is a selectable answer of its own. */
  coreCount(core: EmotionCore): number {
    const sel = this.selected();
    let n = 0;
    for (const g of core.groups) {
      if (sel.has(this.tokenOf(core, g.name))) n++;
      for (const leaf of g.leaves) if (sel.has(this.tokenOf(core, leaf.name))) n++;
    }
    return n;
  }

  toggle(token: string): void {
    const next = new Set(this.selected());
    if (!next.delete(token)) next.add(token);
    this.selected.set(next);
  }

  /** The one word whose gloss is currently open, if any. */
  readonly opened = signal<string | null>(null);

  /** Tap a word's ⓘ: show what it means, in place. Tapping it again — or another
   *  word's ⓘ — closes it, so only ever one gloss is open and reading a word
   *  never selects it. */
  toggleGloss(token: string): void {
    this.opened.update((cur) => (cur === token ? null : token));
  }

  done(): void {
    this.ref.close([...this.selected()]);
  }

  cancel(): void {
    this.ref.close(undefined);
  }
}
