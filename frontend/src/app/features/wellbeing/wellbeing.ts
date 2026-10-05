import {
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatIconModule } from '@angular/material/icon';
import { Sheets } from '@xinutec/ui-scaffold';
import { map } from 'rxjs';

import { localDay } from '../../shared/civil-day';
import { ListState } from '../../shared/list-state';
import { WellbeingCheckin, energyMeta, scoreMeta, toPoints } from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { DayLabel, TrendChart, TrendData, TrendDot } from './trend-chart';
import { WellbeingEntry } from './wellbeing-entry';

interface Day {
  key: string;
  label: string;
  entries: WellbeingDoc[];
}

// One coordinate system for words, dots, rules and day names, so a word is level
// with the dot it names by construction.
const CHART = { w: 300, h: 96, padLeft: 48, padRight: 6, padTop: 8, padBottom: 18 };

const AXIS_X = CHART.padLeft - 6;

/** A day narrower than this gets no weekday name; 14 days falls just below. */
const MIN_DAY_LABEL_W = 30;

/** Readings past each edge that feed the line, so its ends do not wobble as
 *  points scroll in (trend-chart.spec.ts). */
const HALO = 2;

const r1 = (n: number): number => Math.round(n * 10) / 10;

export type TrendWindow = 1 | 7 | 14;

/** One metric, newest first, so a scroll frame is a binary search. */
interface Series {
  times: number[];
  tenths: number[];
}

/** The first index at or below `ms` (strictly below with `strict`). */
function firstAtOrBefore(times: readonly number[], ms: number, strict = false): number {
  let lo = 0;
  let hi = times.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (strict ? times[mid] >= ms : times[mid] > ms) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Check-in strip, mood and energy trends, and the entries by day. */
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
  private sheet = inject(Sheets);

  readonly items = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), { initialValue: false });

  readonly window = signal<TrendWindow>(7);
  readonly windows: readonly { value: TrendWindow; label: string }[] = [
    { value: 1, label: '24h' },
    { value: 7, label: '7d' },
    { value: 14, label: '14d' },
  ];

  private readonly spanMs = computed(() => this.window() * 86_400_000);

  /** Sampled per data change, not live, or the chart would slide under a finger. */
  private readonly now = signal(Date.now());

  /** The dragged-to right edge, or null to follow the latest check-in. */
  private readonly pannedEnd = signal<number | null>(null);

  private readonly panEl = viewChild<ElementRef<HTMLElement>>('pan');

  constructor() {
    effect(() => {
      this.items();
      this.now.set(Date.now());
    });
    // Re-seat the scroller only on a resize or a pin flip: after a zoom,
    // scrollLeft refers to the old rail.
    effect(() => {
      this.panFactor();
      untracked(() => this.reseating.set(true));
    });
    afterRenderEffect(() => {
      const el = this.panEl()?.nativeElement;
      // Read before the element check, or the effect never tracks the width.
      this.panFactor();
      const pinned = this.atNow();
      untracked(() => {
        if (el) {
          const max = el.scrollWidth - el.clientWidth;
          const want =
            pinned || max <= 0
              ? max
              : ((this.endMs() - this.earliestEnd()) / this.pannableMs()) * max;
          // Only when out of step: writing back a derived position echoes forever.
          if (Math.abs(el.scrollLeft - want) > 1) el.scrollLeft = want;
          // Read back: the rail rounds what it was given.
          this.lastLeft = el.scrollLeft;
        }
        // On every path, or the flag sticks and every pan is ignored.
        this.reseating.set(false);
      });
    });
  }

  /** Any check-ins at all: an empty window must not hide the way back. */
  readonly hasAny = computed(() => this.items().length > 0);

  /** Where panning stops. */
  private readonly oldestMs = computed(() => {
    const items = this.items();
    return items.length ? new Date(items[items.length - 1].recordedAt).getTime() : this.now();
  });

  /** Zero when the history fits one window. */
  readonly pannableMs = computed(() => Math.max(0, this.now() - this.spanMs() - this.oldestMs()));

  private readonly earliestEnd = computed(() => this.now() - this.pannableMs());

  /** Clamped, for zooming out while panned far back. */
  readonly endMs = computed(() => {
    const panned = this.pannedEnd();
    if (panned === null) return this.now();
    return Math.min(this.now(), Math.max(this.earliestEnd(), panned));
  });

  readonly atNow = computed(() => this.pannedEnd() === null);

  /** The rail's length in chart widths, as a CSS multiplier. */
  readonly panFactor = computed(() => 1 + this.pannableMs() / this.spanMs());

  private readonly reseating = signal(false);

  /** The last position read or written, to recognise our own write's echo. */
  private lastLeft = Number.NaN;

  /** Within a pixel of the end re-pins to now. */
  onPan(el: HTMLElement): void {
    // Mid-resize the position refers to the old rail.
    if (this.reseating()) return;
    // Measure before writing: a signal write does not update the DOM at once.
    const { scrollLeft, scrollWidth, clientWidth } = el;
    if (scrollLeft === this.lastLeft) return;
    this.lastLeft = scrollLeft;
    const max = scrollWidth - clientWidth;
    const pinned = max <= 0 || max - scrollLeft <= 1;
    this.pannedEnd.set(pinned ? null : this.earliestEnd() + (scrollLeft / max) * this.pannableMs());
  }

  /** Jumps rather than smooth-scrolls, whose events would each unpin it. */
  toNow(): void {
    this.pannedEnd.set(null);
  }

  /** Once panned, "last 7 days" would be false; the range is named. */
  readonly windowLabel = computed(() => {
    const days = this.window();
    if (this.atNow()) return days === 1 ? 'last 24 hours' : `last ${days} days`;
    const day = (ms: number): string =>
      new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
    const end = this.endMs();
    return `${day(end - this.spanMs())} – ${day(end)}`;
  });

  readonly days = computed<Day[]>(() => {
    const groups = new Map<string, Day>();
    for (const e of this.items()) {
      const d = new Date(e.recordedAt);
      const key = localDay(d);
      let g = groups.get(key);
      if (!g) {
        g = { key, label: this.dayLabel(d), entries: [] };
        groups.set(key, g);
      }
      g.entries.push(e);
    }
    return [...groups.values()];
  });

  private readonly moodSeries = computed(() => this.seriesOf((e) => e.scoreTenths));

  private readonly energySeries = computed(() => this.seriesOf((e) => e.energyTenths));

  readonly chart = computed(() => this.buildChart(this.moodSeries()));
  readonly energyChart = computed(() => this.buildChart(this.energySeries()));

  /** Ever recorded, not "has dots here": a chart vanishing mid-pan strands you. */
  readonly hasChart = computed(() => this.moodSeries().times.length > 0);
  readonly hasEnergyChart = computed(() => this.energySeries().times.length > 0);

  readonly emptyWindow = computed(() => this.hasAny() && this.chart().dots.length === 0);

  /** Sorted here: a binary search over an unsorted list plots backwards. */
  private seriesOf(value: (e: WellbeingDoc) => number | null | undefined): Series {
    const readings: { t: number; v: number }[] = [];
    for (const e of this.items()) {
      const reading = value(e);
      if (reading == null) continue; // no reading of this kind on this entry
      readings.push({ t: new Date(e.recordedAt).getTime(), v: reading });
    }
    readings.sort((a, b) => b.t - a.t);
    return { times: readings.map((r) => r.t), tenths: readings.map((r) => r.v) };
  }

  /** Only the window's readings and the halo become dots, so the SVG stays the
   *  same size however long the history. */
  private buildChart(series: Series): TrendData {
    const { w, h, padLeft, padRight, padTop, padBottom } = CHART;
    const plotH = h - padTop - padBottom;
    const spanMs = this.spanMs();
    const endMs = this.endMs();
    const startMs = endMs - spanMs;
    const x = (ms: number): number =>
      padLeft + ((ms - startMs) / spanMs) * (w - padLeft - padRight);
    // The one y rule, for the dots and the axis words alike.
    const y = (level: number): number => r1(padTop + ((5 - level) / 4) * plotH);

    const { times, tenths } = series;
    const newest = firstAtOrBefore(times, endMs);
    const oldest = firstAtOrBefore(times, startMs, true);
    const dot = (i: number): TrendDot => ({
      cx: r1(x(times[i])),
      cy: y(toPoints(tenths[i])),
      fill: scoreMeta(tenths[i]).color,
    });
    const dots: TrendDot[] = [];
    for (let i = oldest - 1; i >= newest; i--) dots.push(dot(i));
    const line: TrendDot[] = [];
    const from = Math.max(0, newest - HALO);
    const to = Math.min(times.length, oldest + HALO);
    for (let i = to - 1; i >= from; i--) line.push(dot(i));

    const bounds = this.midnights(startMs, endMs);
    return {
      w,
      h,
      axisX: AXIS_X,
      plotX: padLeft,
      levelY: [y(5), y(3), y(1)],
      dots,
      line,
      midnights: bounds.map((ms) => r1(x(ms))),
      dayLabels: this.dayLabels([startMs, ...bounds, endMs], x),
    };
  }

  /** Walked with setDate, so a DST change keeps each rule on its day boundary. */
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

  /** A weekday name per day wide enough to hold it. */
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

  time(iso: string): string {
    return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  /** By ulid: the entry may not have reached the server yet. */
  editByKey(ulid: string): void {
    this.sheet.open(WellbeingEntry, { data: { ulid } });
  }

  edit(entry: WellbeingDoc): void {
    this.sheet.open(WellbeingEntry, { data: { ulid: entry.ulid } });
  }

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
