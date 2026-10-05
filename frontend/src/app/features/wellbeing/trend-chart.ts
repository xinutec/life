import { Component, computed, input } from '@angular/core';

/** `fill` agrees with the dot's height, blended for a half-step. */
export interface TrendDot {
  cx: number;
  cy: number;
  fill: string;
}

const r1 = (n: number): number => Math.round(n * 10) / 10;

/** A monotone cubic through x-ascending dots (d3's curveMonotoneX): it never
 *  overshoots, which on a bounded scale would draw moods never logged. */
export function monotonePath(dots: readonly TrendDot[]): string {
  const n = dots.length;
  if (n === 0) return '';
  if (n === 1) return `M${r1(dots[0].cx)},${r1(dots[0].cy)}`;
  const x = dots.map((d) => d.cx);
  const y = dots.map((d) => d.cy);
  const dx: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    dx[i] = x[i + 1] - x[i] || 1e-6;
    m[i] = (y[i + 1] - y[i]) / dx[i];
  }
  // Extrema flatten to 0, the no-overshoot rule.
  const t = new Array<number>(n);
  t[0] = m[0];
  t[n - 1] = m[n - 2];
  for (let i = 1; i < n - 1; i++) {
    t[i] = m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2;
  }
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

export interface DayLabel {
  x: number;
  text: string;
}

/** What the chart draws, all decided by the host. */
export interface TrendData {
  w: number;
  h: number;
  axisX: number;
  plotX: number;
  /** Where a 5, a 3 and a 1 plot; the axis words sit on them. */
  levelY: [number, number, number];
  dots: TrendDot[];
  /** The dots plus the halo beyond each edge; clipped to the plot. */
  line: TrendDot[];
  midnights: number[];
  dayLabels: DayLabel[];
}

/** SVG ids are global and the page draws two charts. */
let nextClipId = 0;

/** Dots on the wellbeing ramp with three axis words. */
@Component({
  selector: 'app-trend-chart',
  templateUrl: './trend-chart.html',
  styleUrl: './trend-chart.scss',
})
export class TrendChart {
  readonly chart = input.required<TrendData>();
  readonly axis = input.required<readonly [string, string, string]>();
  readonly caption = input.required<string>();
  readonly label = input.required<string>();

  readonly clipId = `trend-plot-${nextClipId++}`;

  readonly linePath = computed(() => monotonePath(this.chart().line));
}
