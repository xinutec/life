import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import {
  EMOTION_WHEEL,
  EmotionBlend,
  EmotionCore,
  EmotionNode,
  blendToken,
  emotionBlend,
  emotionColor,
  emotionCore,
  emotionDesc,
  emotionLabel,
  emotionNode,
  emotionToken,
  searchEmotions,
} from '../../shared/emotion-wheel';

export interface EmotionPickerData {
  /** The emotions already on the entry (qualified tokens, or legacy bare words). */
  selected: string[];
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
 *  Two words of one group can be recorded as a single feeling between them (see
 *  [[EmotionBlend]]): choose both, and the footer offers to fuse them. It is only
 *  ever an offer — two words side by side normally means two feelings, and the
 *  picker must not decide for you which of the two you had.
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

  readonly wheel = EMOTION_WHEEL;
  readonly query = signal('');
  // The working set holds canonical tokens; legacy bare words are upgraded here,
  // the single normalisation boundary — every other method deals only in tokens.
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected.map(emotionToken)));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  /** The qualified token for a node — group or leaf — under a given core. */
  tokenOf(core: EmotionCore, name: string): string {
    return `${core.name}/${name}`;
  }

  /** Where a search hit sits: a group shows just its family, a leaf its group too. */
  path(node: EmotionNode): string {
    return node.kind === 'group' ? node.core : `${node.core} › ${node.secondary}`;
  }

  /** The blend a word is part of, if any — the working set holds the blend, not
   *  its halves, so a mosaic word must ask rather than look itself up. */
  private blendOf(token: string): EmotionBlend | null {
    for (const t of this.selected()) {
      const bl = emotionBlend(t);
      if (bl && (bl.a.token === token || bl.b.token === token)) return bl;
    }
    return null;
  }

  /** Chosen on its own, or as half of a blend — either way the word is lit. */
  isSelected(token: string): boolean {
    return this.selected().has(token) || this.blendOf(token) !== null;
  }

  /** Half of a blend: shown at half strength, because it is half an answer. */
  isBlended(token: string): boolean {
    return this.blendOf(token) !== null;
  }

  /** Every blend currently chosen — each offers to split back into two words. */
  readonly blends = computed<readonly EmotionBlend[]>(() =>
    [...this.selected()].map(emotionBlend).filter((b): b is EmotionBlend => b !== null),
  );

  /** Pairs of chosen words that *could* be one feeling instead of two: two leaves
   *  of the same group. Offered, never assumed — "calm AND compassionate" is a
   *  perfectly ordinary answer, and only you know whether you felt one thing or
   *  two. */
  readonly fusable = computed<readonly EmotionBlend[]>(() => {
    const singles = [...this.selected()]
      .map(emotionNode)
      .filter((n): n is EmotionNode => n !== null);
    const out: EmotionBlend[] = [];
    for (let i = 0; i < singles.length; i++) {
      for (let j = i + 1; j < singles.length; j++) {
        const token = blendToken(singles[i], singles[j]);
        if (token) out.push(emotionBlend(token)!);
      }
    }
    return out;
  });

  /** Record the pair as the one feeling it was: the two words leave, the blend
   *  arrives. */
  fuse(bl: EmotionBlend): void {
    const next = new Set(this.selected());
    next.delete(bl.a.token);
    next.delete(bl.b.token);
    next.add(bl.token);
    this.selected.set(next);
  }

  /** Undo a blend: back to the two separate words. */
  split(bl: EmotionBlend): void {
    const next = new Set(this.selected());
    next.delete(bl.token);
    next.add(bl.a.token);
    next.add(bl.b.token);
    this.selected.set(next);
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

  /** How many selected entries fall under a given core (for the family badge) —
   *  groups count too, since a group is a selectable answer of its own, and a
   *  blend counts once, since it is one feeling. */
  coreCount(core: EmotionCore): number {
    let n = 0;
    for (const token of this.selected()) if (emotionCore(token) === core.name) n++;
    return n;
  }

  /** Tapping a word that is half of a blend removes that half — what remains is
   *  the other word, on its own. (A blend can also be split whole, from the
   *  footer.) */
  toggle(token: string): void {
    const next = new Set(this.selected());
    const bl = this.blendOf(token);
    if (bl) {
      next.delete(bl.token);
      next.add(bl.a.token === token ? bl.b.token : bl.a.token);
    } else if (!next.delete(token)) {
      next.add(token);
    }
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
