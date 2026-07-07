import { Component, input } from '@angular/core';

/** One plotted point (SVG user units). `level` is the 1..5 value driving the
 *  shared wellbeing colour ramp. */
export interface TrendDot {
  cx: number;
  cy: number;
  level: number;
}

/** A rendered trend: the viewBox size plus the points to plot. */
export interface TrendData {
  w: number;
  h: number;
  dots: TrendDot[];
}

/** A 14-day wellbeing trend: dots on the shared 1..5 colour ramp, three axis
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
}
