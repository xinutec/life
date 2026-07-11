import { CdkOverlayOrigin, ConnectedPosition, OverlayModule } from '@angular/cdk/overlay';
import { DOCUMENT } from '@angular/common';
import { Component, computed, effect, inject, signal } from '@angular/core';
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

/** The feeling being explained — its identity, word, gloss, and family colour. */
interface Peek {
  token: string;
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
    OverlayModule,
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
  private doc = inject(DOCUMENT);

  constructor() {
    // While a peek is open, dismiss it on any scroll or any tap outside it. Done
    // at the document in the capture phase because the picker scrolls inside the
    // dialog surface — a container CDK's ScrollDispatcher doesn't track, so its
    // scroll strategy never fires — and capture-phase scroll events surface from
    // any container. Taps on the popover or an ⓘ are ignored (the ⓘ toggles
    // itself; a different ⓘ switches). No backdrop, so scrolling stays native.
    effect((onCleanup) => {
      if (!this.peeked()) return;
      const dismiss = (): void => this.dismissPeek();
      const onDown = (e: Event): void => {
        const t = e.target;
        if (t instanceof Element && (t.closest('.peek-pop') || t.closest('.chip-info'))) return;
        this.dismissPeek();
      };
      this.doc.addEventListener('scroll', dismiss, true);
      this.doc.addEventListener('pointerdown', onDown, true);
      onCleanup(() => {
        this.doc.removeEventListener('scroll', dismiss, true);
        this.doc.removeEventListener('pointerdown', onDown, true);
      });
    });
  }

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

  // Tapping a chip's ⓘ opens a small popover anchored to it, giving that
  // feeling's gloss without selecting it — an explicit, visible, accessible
  // affordance (no gesture). Browse stays compact; meaning is one tap away and
  // also inline in search.
  readonly peeked = signal<Peek | null>(null);
  /** The ⓘ the open popover is anchored to (kept once set so the overlay
   *  template always has an origin; visibility is driven by `peeked`). */
  readonly peekOrigin = signal<CdkOverlayOrigin | null>(null);

  /** Prefer above the ⓘ, fall back below; CDK flips/pushes to stay on-screen. */
  readonly peekPositions: ConnectedPosition[] = [
    { originX: 'center', originY: 'top', overlayX: 'center', overlayY: 'bottom', offsetY: -8 },
    { originX: 'center', originY: 'bottom', overlayX: 'center', overlayY: 'top', offsetY: 8 },
  ];

  /** Tap the ⓘ: explain this feeling — or dismiss if it's already the one shown. */
  peek(core: EmotionCore, leaf: EmotionLeafDef, origin: CdkOverlayOrigin): void {
    const token = this.tokenOf(core, leaf.name);
    if (this.peeked()?.token === token) {
      this.peeked.set(null);
      return;
    }
    this.peekOrigin.set(origin);
    this.peeked.set({ token, name: leaf.name, desc: leaf.desc, color: core.color });
  }

  dismissPeek(): void {
    this.peeked.set(null);
  }

  done(): void {
    this.ref.close([...this.selected()]);
  }

  cancel(): void {
    this.ref.close(undefined);
  }
}
