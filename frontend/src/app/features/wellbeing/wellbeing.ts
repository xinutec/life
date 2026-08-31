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
import { MatBottomSheet, MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatIconModule } from '@angular/material/icon';
import { map } from 'rxjs';

import { ListState } from '../../shared/list-state';
import {
  WellbeingCheckin,
  energyMeta,
  scoreMeta,
  toPoints,
} from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { DayLabel, TrendChart, TrendData, TrendDot } from './trend-chart';
import { WellbeingEntry } from './wellbeing-entry';

interface Day {
  key: string;
  label: string;
  entries: WellbeingDoc[];
}

// The whole chart lives in ONE coordinate system: the axis words, the dots, the
// midnight rules and the weekday names are all placed in these viewBox units. The
// words used to be CSS-positioned beside the svg, which is what let them drift off
// the levels they name and (once absolutely positioned) slide off the screen —
// two geometries that only agreed by luck. padLeft is the strip the words sit in;
// padBottom the strip the weekday names sit in.
const CHART = { w: 300, h: 96, padLeft: 48, padRight: 6, padTop: 8, padBottom: 18 };

/** Where the axis words end (right-aligned against the plot). */
const AXIS_X = CHART.padLeft - 6;

/** How wide a day must render (SVG user units) before it gets a weekday name.
 *  A 3-letter word at the .day-name font is ~18 units, so this leaves clear air
 *  either side. At 14 days a day is ~20.6 units — deliberately below the bar, as
 *  fourteen names that nearly touch read as a smear rather than as labels. */
const MIN_DAY_LABEL_W = 30;

/** How many readings beyond each edge of the window feed the trend line.
 *
 *  Not zero: with none the line stops at the last visible dot, and the data looks
 *  like it ends where the screen does. Not one either — monotonePath builds the
 *  tangent at a point from the secants on *both* sides of it, so the curve's shape
 *  near the edge would keep changing as points scrolled in and out, and the line
 *  would visibly wobble under the finger. Two makes every drawn segment identical
 *  to what the whole history draws there, which trend-chart.spec.ts asserts.
 *
 *  Strictly, the monotonicity clamp is a left-to-right pass, so a cascade of
 *  clamps could in principle still reach in from further out. If wobble ever does
 *  show, widen this — don't go hunting for a bug in the curve. */
const HALO = 2;

const r1 = (n: number): number => Math.round(n * 10) / 10;

/** The selectable trend windows, in days. */
export type TrendWindow = 1 | 7 | 14;

/** One metric's history: instants and readings, newest first, holding only the
 *  entries that actually recorded it.
 *
 *  Precomputed per data change so a scroll frame costs a binary search and a
 *  slice — never a walk of the whole history, and never a Date parse per entry
 *  per frame. Split per metric rather than shared, because the halo has to be two
 *  *readings* either side: most check-ins record no energy, so an index halo taken
 *  over all entries could contain no energy reading at all and the energy line
 *  would still stop at the window edge. */
interface Series {
  times: number[];
  tenths: number[];
}

/** In a newest-first list of instants, the first index at or below `ms` — or
 *  strictly below it, which is what the older edge of a half-open window wants.
 *  Returns `times.length` when every reading is newer. */
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

/** Local calendar day key (YYYY-MM-DD) for grouping. */
function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Wellbeing history: a one-tap check-in strip, a trend chart (7 days by
 *  default, 24h/7d/14d selectable), and a day-by-day timeline of entries.
 *  Tapping an entry opens the edit sheet. */
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

  /** Trend window (days) — the zoom. The charts recompute when this changes. */
  readonly window = signal<TrendWindow>(7);
  readonly windows: readonly { value: TrendWindow; label: string }[] = [
    { value: 1, label: '24h' },
    { value: 7, label: '7d' },
    { value: 14, label: '14d' },
  ];

  /** The visible span. */
  private readonly spanMs = computed(() => this.window() * 86_400_000);

  /** "Now" — sampled once per data change rather than read live. The pan maps a
   *  scroll position onto a time range, and a `now` that moved between two reads
   *  would slide the chart under the finger. */
  private readonly now = signal(Date.now());

  /** Where the window's right edge has been dragged to, or null while pinned to
   *  now. The null is the difference between "stay where I scrolled to" and
   *  "follow the latest check-in", and both are wanted. */
  private readonly pannedEnd = signal<number | null>(null);

  private readonly panEl = viewChild<ElementRef<HTMLElement>>('pan');

  constructor() {
    // Re-sample now whenever the data changes, so a check-in logged while the
    // page is open extends the pannable range instead of falling off its end.
    effect(() => {
      this.items();
      this.now.set(Date.now());
    });
    // Seat the scroller where the model says the window is. After render, because
    // the rail's width only exists once --pan-factor has been applied, and a
    // scrollLeft written against the old width lands in the wrong place.
    //
    // Runs when the rail is RESIZED (a zoom change, or new data widening the
    // range) or the pin flips — not on every pan, which is the user's to drive.
    // Resizing is the case that bites: the window's end is a timestamp and
    // survives a zoom change, so the charts still look right, while scrollLeft
    // silently still refers to the old rail. Left alone, the next touch reads that
    // stale position against the new width and teleports the window.
    // A resize has happened and the scroller has not been put right yet. Set
    // where the width changes, cleared by the re-seat below; `onPan` ignores
    // scrolls while it holds, because a scroll arriving in that gap is
    // describing the OLD rail and nobody chose the position it reports.
    effect(() => {
      this.panFactor();
      untracked(() => this.reseating.set(true));
    });
    afterRenderEffect(() => {
      const el = this.panEl()?.nativeElement;
      // Read BEFORE the element check, or an effect that runs once without a
      // rail never tracks the width and stops re-seating for good.
      this.panFactor();
      const pinned = this.atNow();
      untracked(() => {
        if (el) {
          const max = el.scrollWidth - el.clientWidth;
          const want =
            pinned || max <= 0
              ? max
              : ((this.endMs() - this.earliestEnd()) / this.pannableMs()) * max;
          // Only when genuinely out of step. Writing back a position we just
          // derived FROM scrollLeft fires another scroll event, and the two
          // chase each other's rounding.
          if (Math.abs(el.scrollLeft - want) > 1) el.scrollLeft = want;
        }
        // ⚠ On EVERY path, including the one with no rail. An earlier attempt
        // cleared this only after seating, behind an early return — the flag
        // stuck set, every pan was ignored, and the failure rate went from
        // 2-in-10 to 7-in-10. Measured both ways (#1293).
        this.reseating.set(false);
      });
    });
  }

  /** Any check-ins at all — gates the window toggle so it never vanishes just
   *  because the *selected* window happens to be empty (which would strand the
   *  user with no way back to a wider one). */
  readonly hasAny = computed(() => this.items().length > 0);

  /** The oldest check-in — where panning stops. "Infinite" scrolling here means
   *  no fixed window, not endless empty space before the first entry. */
  private readonly oldestMs = computed(() => {
    const items = this.items();
    return items.length ? new Date(items[items.length - 1].recordedAt).getTime() : this.now();
  });

  /** How far back the window's right edge can travel. Zero when the whole history
   *  already fits one window — the charts then don't scroll at all. */
  readonly pannableMs = computed(() =>
    Math.max(0, this.now() - this.spanMs() - this.oldestMs()),
  );

  /** The left-most position of the window's right edge. */
  private readonly earliestEnd = computed(() => this.now() - this.pannableMs());

  /** The window's right edge, clamped: zooming out while panned far back would
   *  otherwise leave the end before its own limit. */
  readonly endMs = computed(() => {
    const panned = this.pannedEnd();
    if (panned === null) return this.now();
    return Math.min(this.now(), Math.max(this.earliestEnd(), panned));
  });

  /** Pinned to the latest check-in rather than parked in the past. */
  readonly atNow = computed(() => this.pannedEnd() === null);

  /** The scroll rail's length as a multiple of the charts' own width: one screen
   *  per window, plus however many more the history covers. Handed to CSS as a
   *  multiplier, so nothing has to measure the chart to size it. */
  readonly panFactor = computed(() => 1 + this.pannableMs() / this.spanMs());

  /** Map the scroller's position onto the window's right edge.
   *
   *  Within a pixel of the end re-pins to now, so a flick to the right edge
   *  starts following new check-ins again rather than freezing the window a few
   *  seconds short of them. */
  /** True between a rail resize and the re-seat that answers it. */
  private readonly reseating = signal(false);

  onPan(el: HTMLElement): void {
    // Mid-resize the scroller still refers to the old rail; reading this
    // position against the new width drags the window a day sideways. The
    // re-seat decides where it sits, not this.
    if (this.reseating()) return;
    // Every measurement taken before the write, and one write: a signal write
    // schedules change detection rather than performing it, so a scrollLeft read
    // after one measures the DOM as it was before.
    const { scrollLeft, scrollWidth, clientWidth } = el;
    const max = scrollWidth - clientWidth;
    const pinned = max <= 0 || max - scrollLeft <= 1;
    this.pannedEnd.set(
      pinned ? null : this.earliestEnd() + (scrollLeft / max) * this.pannableMs(),
    );
  }

  /** Back to the latest check-in — without it, panning back a year is a one-way
   *  trip. Jumps rather than smooth-scrolls: the intermediate scroll events of a
   *  smooth scroll would each read as a fresh pan and unpin it again. */
  toNow(): void {
    this.pannedEnd.set(null);
  }

  /** Human phrase for what is on screen. Once panned, "last 7 days" is a lie, so
   *  the shown range is named instead. */
  readonly windowLabel = computed(() => {
    const days = this.window();
    if (this.atNow()) return days === 1 ? 'last 24 hours' : `last ${days} days`;
    const day = (ms: number): string =>
      new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
    const end = this.endMs();
    return `${day(end - this.spanMs())} – ${day(end)}`;
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

  private readonly moodSeries = computed(() => this.seriesOf((e) => e.scoreTenths));

  /** The optional energy reading — like mood, higher (energetic) sits at the top
   *  and a rising line reads as improving. */
  private readonly energySeries = computed(() => this.seriesOf((e) => e.energyTenths));

  /** The mood trend over the visible window: a dot per reading, x = its position
   *  in time across the window, y = score, joined by a smooth line. */
  readonly chart = computed(() => this.buildChart(this.moodSeries()));
  readonly energyChart = computed(() => this.buildChart(this.energySeries()));

  /** Whether the metric was *ever* recorded — deliberately not "does this window
   *  have dots". A chart that vanished when you panned past its data would take
   *  the scroller with it and strand you there, with no way back. */
  readonly hasChart = computed(() => this.moodSeries().times.length > 0);
  readonly hasEnergyChart = computed(() => this.energySeries().times.length > 0);

  /** The window is empty — worth saying, since the charts now stay on screen. */
  readonly emptyWindow = computed(() => this.hasAny() && this.chart().dots.length === 0);

  /** Collect one metric's readings, newest first.
   *
   *  Sorted here rather than taken on trust from the store's `recordedAt desc`
   *  (wellbeing-store.ts): a binary search over a list that turned out not to be
   *  ordered doesn't fail, it quietly plots the window backwards. The sort costs
   *  one pass per data change and none per scroll frame, so the invariant the
   *  search needs is established where the search can see it. */
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

  /** Build a trend over the visible window. x is the reading's true position in
   *  time across the window (so the line reads chronologically); a half-step plots
   *  between two lines and takes a colour to match, so height and colour tell the
   *  same story, as they do for whole readings.
   *
   *  Only the window's own readings, plus the halo, are turned into dots — which
   *  is what keeps the drawn SVG the same size whether the history is a fortnight
   *  or a decade. */
  private buildChart(series: Series): TrendData {
    const { w, h, padLeft, padRight, padTop, padBottom } = CHART;
    const plotH = h - padTop - padBottom;
    // The window is [end - span, end]. Pinned to now it is a true rolling window,
    // so "24h" is literally the last 24 hours (last night's slump and this morning
    // both show) rather than calendar-today; panned, it is wherever it was dragged.
    const spanMs = this.spanMs();
    const endMs = this.endMs();
    const startMs = endMs - spanMs;
    const x = (ms: number): number =>
      padLeft + ((ms - startMs) / spanMs) * (w - padLeft - padRight);
    // The one rule that places a 1..5 reading on the y axis. The dots use it, and
    // so do the three axis words — that's what keeps "awful" level with a 1.
    const y = (level: number): number => r1(padTop + ((5 - level) / 4) * plotH);

    const { times, tenths } = series;
    // Newest-first, so the window is the index range [newest, oldest), and walking
    // it backwards yields the dots x-ascending without a sort.
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

  /** Open the entry just logged from the strip.
   *
   *  Keyed by ulid, which is what the store hands back on add — the row may not
   *  have reached the server yet, and waiting for a server id before a person
   *  can say how they feel would put the network in the middle of a check-in. */
  editByKey(ulid: string): void {
    this.sheet.open(WellbeingEntry, { data: { ulid } });
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
