import { NgTemplateOutlet } from '@angular/common';
import { Component, DestroyRef, computed, inject, signal } from '@angular/core';
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
  /** Which check-in this is — the key its suggestions are remembered under. */
  ulid: string;
  /** The check-in note, if any — what the suggested feelings are read from. */
  note?: string;
}

/** How often to ask again while a suggestion is being computed. Frequent enough
 *  that the answer appears while you are still looking at the wheel, cheap enough
 *  to leave running: the request is a cache lookup until the answer lands. */
const POLL_MS = 2000;

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
  private destroyRef = inject(DestroyRef);

  readonly wheel = EMOTION_WHEEL;
  readonly query = signal('');
  // The working set holds canonical tokens; legacy bare words are upgraded here,
  // the single normalisation boundary — every other method deals only in tokens.
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected.map(emotionToken)));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  // Feelings read from the note by a small local model, shown at the head of the
  // mosaic. Pure enhancement: with no model running, none of this appears and the
  // picker is exactly the wheel it was.
  //
  // Suggestions are remembered per check-in, so a note you have opened before is
  // answered from the cache and they are simply *there*. Change the note and the
  // previous set stays on screen — labelled as belonging to the older wording —
  // rather than blanking out while the new one is worked out: they are usually
  // still close, and an empty space would be a worse answer than a slightly old one.
  readonly suggestions = signal<readonly EmotionNode[]>([]);
  /** The shown suggestions were read from an earlier wording of the note. */
  readonly stale = signal(false);
  /** A model is working on the current wording right now — never merely "we asked". */
  readonly thinking = signal(false);
  /** Seconds it has been working, anchored to the server's clock so closing and
   *  reopening the picker doesn't restart it. */
  readonly thinkingSecs = signal(0);

  private poll?: ReturnType<typeof setTimeout>;
  private tick?: ReturnType<typeof setInterval>;
  // What was already chosen when the picker opened. Frozen deliberately: the
  // server leaves these out so they don't take up suggestion slots, and sending
  // the *live* selection would make a word vanish from the list two seconds
  // after you tapped it — it stays, with a tick, until the picker is reopened.
  private readonly alreadyAtOpen = [...this.selected()];

  constructor() {
    this.destroyRef.onDestroy(() => this.stopWaiting());
    if ((this.data.note ?? '').trim()) this.refresh();
  }

  /** Ask what is known about this note, and keep asking while something better is
   *  being computed. The request is cheap on the server — a lookup, plus queueing
   *  the work the first time — so polling costs little and stops the moment the
   *  answer lands. */
  private refresh(): void {
    const note = (this.data.note ?? '').trim();
    // The candidates are the whole wheel — sending them keeps the server's ranking
    // in lockstep with exactly what the user can pick, with no second copy to drift.
    const candidates = EMOTION_NODES.map((n) => ({ token: n.token, desc: n.desc }));
    this.api
      .suggestEmotions({ ulid: this.data.ulid, note, candidates, already: this.alreadyAtOpen })
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (r) => {
          const nodes = (r?.suggestions ?? [])
            .map(emotionNode)
            .filter((n): n is EmotionNode => !!n);
          // Only ever replace what's on screen with something: a fresh empty answer
          // is real ("nothing in the wheel fits") and replaces; an empty answer
          // while still thinking is just "not yet" and must not wipe the old set.
          if (nodes.length || !r.pending) this.suggestions.set(nodes);
          this.stale.set(r.stale);
          this.thinking.set(r.pending);
          this.thinkingSecs.set(r.thinkingSecs ?? 0);
          if (r.pending) this.keepWaiting();
          else this.stopWaiting();
        },
        // Offline, or the server had nothing to say. Leave whatever is on screen
        // and stop claiming anything is happening.
        error: () => {
          this.thinking.set(false);
          this.stopWaiting();
        },
      });
  }

  /** Poll for the answer, and count the seconds in between so the elapsed time
   *  moves smoothly rather than jumping with each request. */
  private keepWaiting(): void {
    this.stopWaiting();
    this.poll = setTimeout(() => this.refresh(), POLL_MS);
    this.tick = setInterval(() => this.thinkingSecs.update((s) => s + 1), 1000);
  }

  private stopWaiting(): void {
    clearTimeout(this.poll);
    clearInterval(this.tick);
    this.poll = undefined;
    this.tick = undefined;
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
