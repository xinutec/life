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
  /** Tokens, or legacy bare words. */
  selected: string[];
  /** The key its suggestions are remembered under. */
  ulid: string;
  note?: string;
}

/** Cheap: a cache lookup until the answer lands. */
const POLL_MS = 2000;

/** Browse and search the feelings wheel. Closes with the new tokens, or
 *  `undefined` if dismissed. */
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
  // Legacy bare words become tokens here and nowhere else.
  readonly selected = signal<ReadonlySet<string>>(new Set(this.data.selected.map(emotionToken)));

  readonly results = computed(() => searchEmotions(this.query()));
  readonly count = computed(() => this.selected().size);
  readonly selectedList = computed(() => [...this.selected()]);

  // From the local model; after an edit the old set stays, marked stale.
  readonly suggestions = signal<readonly EmotionNode[]>([]);
  readonly stale = signal(false);
  /** Working on the current wording right now, not merely asked. */
  readonly thinking = signal(false);
  /** Anchored to the server's clock, so reopening does not restart it. */
  readonly thinkingSecs = signal(0);

  private poll?: ReturnType<typeof setTimeout>;
  private tick?: ReturnType<typeof setInterval>;
  // Frozen at open: the server leaves these out, and the live selection would
  // make a word vanish seconds after it was tapped.
  private readonly alreadyAtOpen = [...this.selected()];

  constructor() {
    this.destroyRef.onDestroy(() => this.stopWaiting());
    if ((this.data.note ?? '').trim()) this.refresh();
  }

  /** Keep asking while an answer is being computed. */
  private refresh(): void {
    const note = (this.data.note ?? '').trim();
    // The whole wheel, so the server ranks exactly what can be picked.
    const candidates = EMOTION_NODES.map((n) => ({ token: n.token, desc: n.desc }));
    this.api
      .suggestEmotions({ ulid: this.data.ulid, note, candidates, already: this.alreadyAtOpen })
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (r) => {
          const nodes = (r?.suggestions ?? [])
            .map(emotionNode)
            .filter((n): n is EmotionNode => !!n);
          // An empty answer while still thinking is "not yet", not "none".
          if (nodes.length || !r.pending) this.suggestions.set(nodes);
          this.stale.set(r.stale);
          this.thinking.set(r.pending);
          this.thinkingSecs.set(r.thinkingSecs ?? 0);
          if (r.pending) this.keepWaiting();
          else this.stopWaiting();
        },
        error: () => {
          this.thinking.set(false);
          this.stopWaiting();
        },
      });
  }

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

  tokenOf(core: EmotionCore, name: string): string {
    return `${core.name}/${name}`;
  }

  path(node: EmotionNode): string {
    return node.kind === 'group' ? node.core : `${node.core} › ${node.secondary}`;
  }

  isSelected(token: string): boolean {
    return this.selected().has(token);
  }

  color(token: string): string {
    return emotionColor(token);
  }

  label(token: string): string {
    return emotionLabel(token);
  }

  desc(token: string): string {
    return emotionDesc(token);
  }

  /** Both rings. */
  wordCount(core: EmotionCore): number {
    return core.groups.reduce((n, g) => n + g.leaves.length + 1, 0);
  }

  /** Groups count: a group is an answer of its own. */
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

  readonly opened = signal<string | null>(null);

  /** One gloss open at a time; reading a word never selects it. */
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
