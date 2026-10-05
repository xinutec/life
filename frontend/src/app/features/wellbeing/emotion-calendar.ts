/** The emotion history as a calendar: each box filled with the day's families,
 *  with the score range as a bar a proportion would hide. */
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
import { CalendarDay, buildCalendar, dayTitle, tallyAcross } from './emotion-calendar-model';

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const;

const MONTH_NAMES = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const;

/** A fixed scale, so two months stay comparable. */
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

  private readonly docs = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });

  readonly months = computed(() =>
    buildCalendar(this.docs()).map((m) => ({
      ...m,
      label: `${MONTH_NAMES[m.month]} ${m.year}`,
    })),
  );

  private readonly picked = signal<readonly string[]>([]);

  /** Once, when the months first render: a later sync must not yank the page. */
  private jumped = false;

  constructor() {
    afterRenderEffect(() => {
      const ready = this.months().length > 0;
      untracked(() => {
        if (!ready || this.jumped) return;
        this.jumped = true;
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

  readonly selectedTally = computed(() => tallyAcross(this.selected()));

  /** As in the picker; the count is omitted at one. */
  readonly selectedChips = computed(() =>
    this.selectedTally().map(({ token, days }) => ({
      token,
      label: emotionLabel(token),
      cls: `emo emo-${emotionColor(token)}`,
      count: days > 1 ? days : null,
    })),
  );

  /** "3 of 30", not a bare number that could be a score. */
  chipTitle(chip: { label: string; count: number | null }): string {
    const total = this.selected().length;
    const on = chip.count ?? 1;
    return `${chip.label} — on ${on} of ${total} selected day${total === 1 ? '' : 's'}`;
  }

  toggle(day: CalendarDay): void {
    // A day without a check-in has nothing to select.
    if (!day.checkins) return;
    this.picked.update((keys) =>
      keys.includes(day.key) ? keys.filter((k) => k !== day.key) : [...keys, day.key],
    );
  }

  isSelected(day: CalendarDay): boolean {
    return this.picked().includes(day.key);
  }

  clear(): void {
    this.picked.set([]);
  }

  /** Hard-edged bands: blended hues read as a family that is not there. */
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

  rangeBar(day: CalendarDay): { left: number; width: number } | null {
    if (day.scoreLow === null || day.scoreHigh === null || day.spread === 0) return null;
    const span = SCORE_MAX - SCORE_MIN;
    return {
      left: ((day.scoreLow - SCORE_MIN) / span) * 100,
      width: ((day.scoreHigh - day.scoreLow) / span) * 100,
    };
  }

  title(day: CalendarDay): string {
    return dayTitle(day);
  }
}
