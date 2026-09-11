/** The emotion history as a calendar: one box per day, coloured by the families
 *  that day's check-ins named.
 *
 *  Reading it: the FILL is how the day went, as a proportion — a solid box is one
 *  family all day, stripes are a day that moved. The BAR under the number is the
 *  score range, and it is there because the fill alone lies by averaging (see
 *  emotion-calendar-model.ts, which measures the case). A flat day has no bar.
 *
 *  Months read oldest-first and the view scrolls to the end once, like a chat:
 *  time runs one way and you still land on today. Doing it by ordering instead
 *  (newest month first) made time run backwards at one scale and forwards at
 *  the other — see emotion-calendar-model.ts.
 *
 *  Selecting days is not decoration: the selection's tokens are the input a 3D
 *  render of a day takes, so `selected` is the handoff, not a highlight. */
import {
  ChangeDetectionStrategy,
  Component,
  afterRenderEffect,
  computed,
  inject,
  signal,
  untracked,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

import { emotionColor, emotionLabel } from '../../shared/emotion-wheel';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { CalendarDay, buildCalendar, tallyAcross } from './emotion-calendar-model';

/** Monday first, matching the grid the model pads for. */
const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const;

const MONTH_NAMES = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
] as const;

/** The score scale is 10..50 tenths, so a range bar is drawn against that and
 *  not against whatever the visible days happen to span — a bar that rescaled
 *  itself per month would make two different months incomparable. */
const SCORE_MIN = 10;
const SCORE_MAX = 50;

@Component({
  selector: 'app-emotion-calendar',
  standalone: true,
  imports: [MatButtonModule, MatIconModule],
  templateUrl: './emotion-calendar.html',
  styleUrl: './emotion-calendar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EmotionCalendar {
  private readonly store = inject(WellbeingStore);

  readonly weekdays = WEEKDAYS;

  // `items$` is already the live, non-deleted view — RxDB filters tombstones,
  // so there is nothing to re-filter here.
  private readonly docs = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });

  readonly months = computed(() =>
    buildCalendar(this.docs()).map((m) => ({
      ...m,
      label: `${MONTH_NAMES[m.month]} ${m.year}`,
    })),
  );

  private readonly picked = signal<readonly string[]>([]);

  /** Once only. The check-ins arrive asynchronously, so the first render is
   *  empty and there is nothing to scroll to yet — this waits for the render
   *  that has months. It must not re-fire afterwards, or a background sync
   *  would yank the page out from under someone reading July. */
  private jumped = false;

  constructor() {
    afterRenderEffect(() => {
      const ready = this.months().length > 0;
      untracked(() => {
        if (!ready || this.jumped) return;
        this.jumped = true;
        // The document scrolls, not an inner element — measured, rather than
        // assumed from the template.
        const el = document.scrollingElement ?? document.documentElement;
        el.scrollTop = el.scrollHeight;
      });
    });
  }

  readonly selected = computed<readonly CalendarDay[]>(() => {
    const keys = new Set(this.picked());
    if (!keys.size) return [];
    return this.months()
      .flatMap((m) => m.cells)
      .filter((c): c is CalendarDay => !!c && keys.has(c.key));
  });

  /** Every emotion across the selected days, commonest first — what a render of
   *  the selection would be given. */
  readonly selectedTally = computed(() => tallyAcross(this.selected()));

  /** Chips in the app's existing grammar: `emo emo-<family>`, so a word in the
   *  selection looks identical to the same word in the picker.
   *
   *  The count rides along because a wide selection is a list of words with no
   *  weight otherwise: across the whole log that is 76 of them, and knowing
   *  which ones recur is the entire reason to select a stretch rather than a
   *  day. It is omitted at one, where it would only ever read "1". */
  readonly selectedChips = computed(() =>
    this.selectedTally().map(({ token, days }) => ({
      token,
      label: emotionLabel(token),
      cls: `emo emo-${emotionColor(token)}`,
      count: days > 1 ? days : null,
    })),
  );

  /** Spells the count out, because a bare number on a chip could be a score, a
   *  rank or a tally. Says it against the size of the selection so "3" is read
   *  as "3 of 30" rather than as "a lot". */
  chipTitle(chip: { label: string; count: number | null }): string {
    const total = this.selected().length;
    const on = chip.count ?? 1;
    return `${chip.label} — on ${on} of ${total} selected day${total === 1 ? '' : 's'}`;
  }

  toggle(day: CalendarDay): void {
    // A day nobody checked in on has nothing to hand on, so it is not selectable
    // — selecting it would put an empty box in a selection that reads as a set of
    // feelings.
    if (!day.checkins) return;
    this.picked.update((keys) =>
      keys.includes(day.key) ? keys.filter((k) => k !== day.key) : [...keys, day.key],
    );
  }

  isSelected(day: CalendarDay): boolean {
    return this.picked().includes(day.key);
  }

  /** Drops the day SELECTION. Nothing here writes: this component reads
   *  `items$` and owns no other store call, so no path through it can alter a
   *  check-in. Stated because the control was read as deleting feelings, which
   *  is the reading a label has to rule out rather than a comment. */
  clear(): void {
    this.picked.set([]);
  }

  /** `--emo-<color>` stops laid end to end, as a vertical gradient with hard
   *  edges — bands rather than a blend, because six blended hues are mud and a
   *  two-family blend reads as a third family that does not exist. */
  fill(day: CalendarDay): string {
    if (!day.bands.length) return 'transparent';
    const stops: string[] = [];
    let at = 0;
    for (const b of day.bands) {
      const from = at * 100;
      at += b.fraction;
      stops.push(`var(--emo-${b.color}) ${from.toFixed(2)}%`);
      stops.push(`var(--emo-${b.color}) ${(at * 100).toFixed(2)}%`);
    }
    return `linear-gradient(to bottom, ${stops.join(', ')})`;
  }

  /** Range bar geometry as percentages of the box width, on the fixed 10..50
   *  scale. Null when there is nothing to draw. */
  rangeBar(day: CalendarDay): { left: number; width: number } | null {
    if (day.scoreLow === null || day.scoreHigh === null || day.spread === 0) return null;
    const span = SCORE_MAX - SCORE_MIN;
    return {
      left: ((day.scoreLow - SCORE_MIN) / span) * 100,
      width: ((day.scoreHigh - day.scoreLow) / span) * 100,
    };
  }

  title(day: CalendarDay): string {
    if (!day.checkins) return `${day.key} — no check-in`;
    const fams = day.bands.map((b) => `${b.core} ${Math.round(b.fraction * 100)}%`).join(', ');
    // "no score" rather than a number, for a day whose readings carried none —
    // this printed `score NaN–NaN` before, which reads as a broken app rather
    // than as a doc that predates the field.
    const range =
      day.scoreLow === null || day.scoreHigh === null
        ? 'no score'
        : day.spread === 0
          ? `score ${day.scoreLow}`
          : `score ${day.scoreLow}–${day.scoreHigh}`;
    const reads = `${day.checkins} check-in${day.checkins === 1 ? '' : 's'}`;
    return `${day.key} — ${reads}, ${range}${fams ? `, ${fams}` : ', nothing tagged'}`;
  }
}
