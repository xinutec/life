import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { MatBottomSheet, MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatIconModule } from '@angular/material/icon';
import { map } from 'rxjs';

import { ListState } from '../../shared/list-state';
import { WellbeingCheckin, energyMeta, scoreMeta } from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { DayLabel, TrendChart, TrendData, TrendDot } from './trend-chart';
import { WellbeingEntry } from './wellbeing-entry';

interface Day {
  key: string;
  label: string;
  entries: WellbeingDoc[];
}

// padBottom reserves the strip under the plot for the weekday names, so a "Mon"
// never sits on top of a bad day's dot at the bottom of the scale. Keep it in
// step with .axis::after in trend-chart.scss, which holds the same strip back
// from the axis words: that spacer is (padBottom - padTop) / h.
const CHART = { w: 300, h: 96, padX: 6, padTop: 8, padBottom: 18 };

/** How wide a day must render (SVG user units) before it gets a weekday name.
 *  A 3-letter word at the .day-name font is ~18 units, so this leaves clear air
 *  either side. At 14 days a day is ~20.6 units — deliberately below the bar, as
 *  fourteen names that nearly touch read as a smear rather than as labels. */
const MIN_DAY_LABEL_W = 30;

const r1 = (n: number): number => Math.round(n * 10) / 10;

/** The selectable trend windows, in days. */
export type TrendWindow = 1 | 7 | 14;

/** Local calendar day key (YYYY-MM-DD) for grouping. */
function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Wellbeing history: a one-tap check-in strip, a 14-day trend, and a day-by-day
 *  timeline of entries. Tapping an entry opens the edit sheet. */
@Component({
  selector: 'app-wellbeing',
  templateUrl: './wellbeing.html',
  styleUrl: './wellbeing.scss',
  imports: [
    MatButtonModule,
    MatButtonToggleModule,
    MatIconModule,
    MatBottomSheetModule,
    ListState,
    WellbeingCheckin,
    TrendChart,
  ],
})
export class Wellbeing {
  private store = inject(WellbeingStore);
  private sheet = inject(MatBottomSheet);

  readonly items = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), { initialValue: false });

  /** Trend window (days). The charts recompute when this changes. */
  readonly window = signal<TrendWindow>(14);
  readonly windows: readonly { value: TrendWindow; label: string }[] = [
    { value: 1, label: '24h' },
    { value: 7, label: '7d' },
    { value: 14, label: '14d' },
  ];

  /** Any check-ins at all — gates the window toggle so it never vanishes just
   *  because the *selected* window happens to be empty (which would strand the
   *  user with no way back to a wider one). */
  readonly hasAny = computed(() => this.items().length > 0);

  /** Human phrase for the current window, for captions/labels. */
  readonly windowLabel = computed(() => {
    const d = this.window();
    return d === 1 ? 'last 24 hours' : `last ${d} days`;
  });

  /** Entries grouped by local day, newest day first (items$ is already desc). */
  readonly days = computed<Day[]>(() => {
    const groups = new Map<string, Day>();
    for (const e of this.items()) {
      const d = new Date(e.recordedAt);
      const key = dayKey(d);
      let g = groups.get(key);
      if (!g) {
        g = { key, label: this.dayLabel(d), entries: [] };
        groups.set(key, g);
      }
      g.entries.push(e);
    }
    return [...groups.values()];
  });

  /** The mood trend over the selected window: a dot per entry, x = its position in
   *  time across the window, y = score, joined by a smooth line. */
  readonly chart = computed(() => this.buildChart((e) => e.score));
  readonly hasChart = computed(() => this.chart().dots.length > 0);

  /** The same trend for the optional energy reading — like mood, higher
   *  (energetic) sits at the top and a rising line reads as improving. Only
   *  entries that recorded one contribute, so it's absent until there's data. */
  readonly energyChart = computed(() => this.buildChart((e) => e.energy));
  readonly hasEnergyChart = computed(() => this.energyChart().dots.length > 0);

  /** Build a trend from a 1..5 accessor over the current window. x is the entry's
   *  true position in time across the window (so the line reads chronologically);
   *  entries with no reading of this kind, or outside the window, are skipped.
   *  Dots come out x-ascending so the connecting line joins them in time order. */
  private buildChart(value: (e: WellbeingDoc) => number | null | undefined): TrendData {
    const { w, h, padX, padTop, padBottom } = CHART;
    const days = this.window();
    const plotH = h - padTop - padBottom;
    // A true rolling window ending now: [now - days·24h, now]. So "24h" is
    // literally the last 24 hours (last night's slump and this morning both
    // show), not calendar-today — and the newest entry sits at the right edge.
    const spanMs = days * 86_400_000;
    const startMs = Date.now() - spanMs;
    const endMs = startMs + spanMs;
    const x = (ms: number): number => padX + ((ms - startMs) / spanMs) * (w - 2 * padX);
    const dots: TrendDot[] = [];
    for (const e of this.items()) {
      const level = value(e);
      if (level == null) continue; // no reading of this kind on this entry
      const t = new Date(e.recordedAt).getTime();
      if (t < startMs || t > endMs) continue; // outside the window
      const cy = padTop + ((5 - level) / 4) * plotH;
      dots.push({ cx: r1(x(t)), cy: r1(cy), level });
    }
    dots.sort((a, b) => a.cx - b.cx);
    const bounds = this.midnights(startMs, endMs);
    return {
      w,
      h,
      midY: padTop + plotH / 2,
      dots,
      midnights: bounds.map((ms) => r1(x(ms))),
      dayLabels: this.dayLabels([startMs, ...bounds, endMs], x),
    };
  }

  /** Local midnights inside the window. Walked with setDate rather than adding
   *  86 400 000 ms so a DST change keeps each rule on the day boundary the
   *  entries either side of it are actually stamped against. */
  private midnights(startMs: number, endMs: number): number[] {
    const out: number[] = [];
    const d = new Date(startMs);
    d.setHours(0, 0, 0, 0);
    d.setDate(d.getDate() + 1); // the first midnight after the window opens
    while (d.getTime() < endMs) {
      out.push(d.getTime());
      d.setDate(d.getDate() + 1);
    }
    return out;
  }

  /** A weekday name centred in each day, from the window's day boundaries.
   *  Labelled only where the day is wide enough to hold the word — measured
   *  against the day's own rendered width, so it's the chart that decides, not
   *  the window setting: the part-days at either edge drop their label when the
   *  window opens late in the day, and 14d stays clean while 7d and 24h label. */
  private dayLabels(bounds: number[], x: (ms: number) => number): DayLabel[] {
    const out: DayLabel[] = [];
    for (let i = 0; i < bounds.length - 1; i++) {
      const [from, to] = [bounds[i], bounds[i + 1]];
      if (x(to) - x(from) < MIN_DAY_LABEL_W) continue; // too narrow for the word
      const mid = new Date(from + (to - from) / 2);
      out.push({
        x: r1(x(from) + (x(to) - x(from)) / 2),
        text: mid.toLocaleDateString(undefined, { weekday: 'short' }),
      });
    }
    return out;
  }

  meta(score: number) {
    return scoreMeta(score);
  }

  energyMeta(energy: number) {
    return energyMeta(energy);
  }

  /** "14:05" — the entry's local clock time. */
  time(iso: string): string {
    return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  edit(entry: WellbeingDoc): void {
    this.sheet.open(WellbeingEntry, { data: { ulid: entry.ulid } });
  }

  /** "Today" / "Yesterday" / "Sat 5 Jul". */
  private dayLabel(d: Date): string {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const that = new Date(d);
    that.setHours(0, 0, 0, 0);
    const diff = Math.round((today.getTime() - that.getTime()) / 86_400_000);
    if (diff === 0) return 'Today';
    if (diff === 1) return 'Yesterday';
    return d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
  }
}
