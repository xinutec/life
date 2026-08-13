import { Component, computed, input } from '@angular/core';

/** One plotted point (SVG user units). `fill` is a CSS colour off the shared
 *  wellbeing ramp — a rung of it for a whole reading, a blend of two rungs for a
 *  half-step, so the dot's colour agrees with its height. */
export interface TrendDot {
  cx: number;
  cy: number;
  fill: string;
}

const r1 = (n: number): number => Math.round(n * 10) / 10;

/** Smooth monotone-cubic path (SVG `d`) through the dots, which must be x-ascending.
 *  Matches d3's `curveMonotoneX` (Fritsch–Carlson): C¹-continuous yet guaranteed
 *  not to overshoot a data point — vital on a bounded 1..5 scale, where an
 *  overshooting curve would dip below the worst mood ever logged (or above the
 *  best) and read as data that never happened. Fewer than two dots → no line. */
export function monotonePath(dots: readonly TrendDot[]): string {
  const n = dots.length;
  if (n === 0) return '';
  if (n === 1) return `M${r1(dots[0].cx)},${r1(dots[0].cy)}`;
  const x = dots.map((d) => d.cx);
  const y = dots.map((d) => d.cy);
  // Secant slope of each segment (guard a zero dx so co-timed entries don't /0).
  const dx: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    dx[i] = x[i + 1] - x[i] || 1e-6;
    m[i] = (y[i + 1] - y[i]) / dx[i];
  }
  // Tangents: interior extrema flatten to 0 (the no-overshoot rule), ends match
  // their one secant, the rest average the two neighbouring secants.
  const t = new Array<number>(n);
  t[0] = m[0];
  t[n - 1] = m[n - 2];
  for (let i = 1; i < n - 1; i++) {
    t[i] = m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2;
  }
  // Clamp each tangent pair into the monotonicity circle (‖(α,β)‖ ≤ 3).
  for (let i = 0; i < n - 1; i++) {
    if (m[i] === 0) {
      t[i] = 0;
      t[i + 1] = 0;
      continue;
    }
    const a = t[i] / m[i];
    const b = t[i + 1] / m[i];
    const s = a * a + b * b;
    if (s > 9) {
      const f = 3 / Math.sqrt(s);
      t[i] = f * a * m[i];
      t[i + 1] = f * b * m[i];
    }
  }
  // Hermite → cubic Bézier control points, a third of the way along each segment.
  let d = `M${r1(x[0])},${r1(y[0])}`;
  for (let i = 0; i < n - 1; i++) {
    const h = dx[i];
    d +=
      `C${r1(x[i] + h / 3)},${r1(y[i] + (t[i] * h) / 3)} ` +
      `${r1(x[i + 1] - h / 3)},${r1(y[i + 1] - (t[i + 1] * h) / 3)} ` +
      `${r1(x[i + 1])},${r1(y[i + 1])}`;
  }
  return d;
}

/** A weekday name ("Mon") centred in its day, below the plot. */
export interface DayLabel {
  x: number;
  text: string;
}

/** A rendered trend: the viewBox size, the points to plot, the x of every local
 *  midnight inside the window — drawn as faint rules, so a dip reads as "that
 *  was Tuesday" rather than "that was somewhere in the middle" — and the weekday
 *  names for those days wide enough to be worth naming (so the host decides what
 *  fits; the chart just draws what it's given). */
export interface TrendData {
  w: number;
  h: number;
  /** x the axis words are right-aligned against, inside the plot's own coordinates. */
  axisX: number;
  /** x the plot starts at — where the mid line begins and the clip opens, just
   *  right of the axis words. */
  plotX: number;
  /** y of the top, middle and bottom of the scale — where a 5, a 3 and a 1 plot.
   *  The mid line is drawn on the middle one and the three axis words sit on all
   *  three, so a word is level with the dot it names by construction. */
  levelY: [number, number, number];
  /** The readings inside the window — one circle each. x-ascending. */
  dots: TrendDot[];
  /** Every point the trend line passes through, x-ascending: the `dots` plus a
   *  couple of readings beyond each edge, so the curve enters and leaves the
   *  viewport drawn the way the whole history would draw it rather than stopping
   *  dead at the last visible dot. Those outer points lie outside the plot — which
   *  is why the line is clipped, or it would run over the axis words. */
  line: TrendDot[];
  midnights: number[];
  dayLabels: DayLabel[];
}

/** Per-instance suffix for the clip path's id. SVG ids are document-global and
 *  this page draws two charts, so a fixed id would have the second silently
 *  reuse — or overwrite — the first one's clip. */
let nextClipId = 0;

/** A wellbeing trend chart: dots on the shared 1..5 colour ramp, three axis
 *  words down the left edge, a caption below. Purely presentational — the host
 *  computes the dots (mood, energy, or any future 1..5 axis) and supplies the
 *  labels, so the two charts share one implementation. */
@Component({
  selector: 'app-trend-chart',
  templateUrl: './trend-chart.html',
  styleUrl: './trend-chart.scss',
})
export class TrendChart {
  readonly chart = input.required<TrendData>();
  /** Axis words top → bottom (e.g. ['great', 'okay', 'awful']). */
  readonly axis = input.required<readonly [string, string, string]>();
  readonly caption = input.required<string>();
  /** Accessible description of the whole plot. */
  readonly label = input.required<string>();

  readonly clipId = `trend-plot-${nextClipId++}`;

  /** The smooth trend line; empty (hidden) below two points. Drawn through the
   *  halo as well as the visible dots, and clipped to the plot. */
  readonly linePath = computed(() => monotonePath(this.chart().line));
}
