import { describe, expect, it } from 'vitest';

import { monotonePath, TrendDot } from './trend-chart';

const dot = (cx: number, cy: number, level = 3): TrendDot => ({ cx, cy, level });

/** All (x,y) numbers in an SVG path, as flat pairs (M x y, then C … x y ×n). */
function coords(d: string): { x: number; y: number }[] {
  const nums = (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);
  const out: { x: number; y: number }[] = [];
  for (let i = 0; i + 1 < nums.length; i += 2) out.push({ x: nums[i], y: nums[i + 1] });
  return out;
}

describe('monotonePath', () => {
  it('is empty below one point (nothing to draw)', () => {
    expect(monotonePath([])).toBe('');
  });

  it('is a lone move for a single point', () => {
    expect(monotonePath([dot(5, 40)])).toBe('M5,40');
  });

  it('emits one cubic segment per gap and hits every data point', () => {
    const dots = [dot(0, 50), dot(10, 20), dot(20, 30), dot(30, 10)];
    const d = monotonePath(dots);
    expect((d.match(/C/g) ?? []).length).toBe(dots.length - 1);
    // Curve starts at the first point and passes through each in turn.
    expect(d.startsWith('M0,50')).toBe(true);
    for (const p of dots) expect(d).toContain(`${p.cx},${p.cy}`);
  });

  it('never overshoots the data range (bounded 1..5 scale must not invent a dip)', () => {
    // A valley then a peak — the classic overshoot trap for a naive spline.
    const dots = [dot(0, 40), dot(10, 80), dot(20, 20), dot(30, 60)];
    const ys = dots.map((p) => p.cy);
    const lo = Math.min(...ys);
    const hi = Math.max(...ys);
    // Every control point and vertex stays within the data's y-range (± rounding).
    for (const { y } of coords(monotonePath(dots))) {
      expect(y).toBeGreaterThanOrEqual(lo - 0.1);
      expect(y).toBeLessThanOrEqual(hi + 0.1);
    }
  });

  it('handles co-timed points without dividing by zero', () => {
    // Two entries at the same x (same instant) must not produce NaN/Infinity.
    const d = monotonePath([dot(10, 30), dot(10, 50), dot(20, 20)]);
    expect(d).not.toMatch(/NaN|Infinity/);
  });
});
