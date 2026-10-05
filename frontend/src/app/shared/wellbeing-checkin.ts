import { Component, DestroyRef, inject, input, output, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

import { Feedback } from './feedback';
import { WellbeingStore } from '../sync/wellbeing-store';

/** Readings are tenths of a point (10..50): a 35 is a 3.5, between two faces. */
export const TENTHS_PER_POINT = 10;
export const HALF_STEP_TENTHS = 5;

/** 35 → 3.5; the one place the scale is undone. */
export function toPoints(tenths: number): number {
  return tenths / TENTHS_PER_POINT;
}

/** 4 → 40. */
export function toTenths(points: number): number {
  return points * TENTHS_PER_POINT;
}

export function isHalfStep(tenths: number): boolean {
  return tenths % TENTHS_PER_POINT !== 0;
}

/** 3 and 4 → 35. */
export function midpoint(a: number, b: number): number {
  return (toTenths(a) + toTenths(b)) / 2;
}

/** The faces a reading lights: one for a 4, both neighbours for a 3.5. */
export function facesOf(tenths: number): number[] {
  const points = toPoints(tenths);
  return isHalfStep(tenths) ? [Math.floor(points), Math.ceil(points)] : [points];
}

/** The reading after tapping `face`: a neighbour of a whole makes the half-step,
 *  anything else goes to that face. Null for the lone lit face, which mood keeps
 *  and energy clears. */
export function nextReading(now: number | null | undefined, face: number): number | null {
  if (now == null || isHalfStep(now)) return toTenths(face);
  const current = toPoints(now);
  if (current === face) return null;
  return Math.abs(current - face) === 1 ? midpoint(current, face) : toTenths(face);
}

/** The five mood levels. */
export const WELLBEING_SCORES: readonly { score: number; label: string; icon: string }[] = [
  { score: 1, label: 'awful', icon: 'sentiment_very_dissatisfied' },
  { score: 2, label: 'low', icon: 'sentiment_dissatisfied' },
  { score: 3, label: 'okay', icon: 'sentiment_neutral' },
  { score: 4, label: 'good', icon: 'sentiment_satisfied' },
  { score: 5, label: 'great', icon: 'sentiment_very_satisfied' },
];

/** The five energy levels; higher is better, as for mood. */
export const ENERGY_LEVELS: readonly { energy: number; label: string; icon: string }[] = [
  { energy: 1, label: 'drained', icon: 'battery_alert' },
  { energy: 2, label: 'low', icon: 'battery_2_bar' },
  { energy: 3, label: 'okay', icon: 'battery_3_bar' },
  { energy: 4, label: 'good', icon: 'battery_5_bar' },
  { energy: 5, label: 'energetic', icon: 'battery_full' },
];

export interface LevelMeta {
  label: string;
  icon: string;
  /** A rung of the ramp, or for a half-step the blend of two. */
  color: string;
}

/** A half-step takes the lower rung's icon, the level surely reached, and both
 *  rungs' words ("okay–good"). */
function levelMeta(
  tenths: number,
  rungs: readonly { label: string; icon: string }[],
  ramp: string,
): LevelMeta {
  const points = toPoints(tenths);
  const lower = rungs[Math.max(0, Math.min(rungs.length - 1, Math.floor(points) - 1))];
  if (!isHalfStep(tenths)) {
    return { label: lower.label, icon: lower.icon, color: `var(--${ramp}-${Math.round(points)})` };
  }
  const upper = rungs[Math.max(0, Math.min(rungs.length - 1, Math.ceil(points) - 1))];
  return {
    label: `${lower.label}–${upper.label}`,
    icon: lower.icon,
    color: `color-mix(in srgb, var(--${ramp}-${Math.floor(points)}) 50%, var(--${ramp}-${Math.ceil(points)}))`,
  };
}

export function scoreMeta(tenths: number): LevelMeta {
  return levelMeta(tenths, WELLBEING_SCORES, 'wb-score');
}

export function energyMeta(tenths: number): LevelMeta {
  return levelMeta(tenths, ENERGY_LEVELS, 'wb-score');
}

/** How long after a tap an adjacent tap amends it ("4… no, a bit below"). */
const AMEND_WINDOW_MS = 60_000;

/** The one-tap mood check-in. An adjacent tap within the amend window makes it
 *  a half-step. */
@Component({
  selector: 'app-wellbeing-checkin',
  templateUrl: './wellbeing-checkin.html',
  styleUrl: './wellbeing-checkin.scss',
  imports: [MatButtonModule, MatIconModule],
})
export class WellbeingCheckin {
  private store = inject(WellbeingStore);
  private feedback = inject(Feedback);

  readonly logged = output<void>();

  /** Only for a host that can open the entry. */
  readonly showDetail = input(false);
  readonly detail = output<string>();

  /** The entry just logged, while the amend window is open. Not an auto-opened
   *  sheet, which would cover the faces the amend tap needs. */
  readonly justLogged = signal<string | null>(null);

  readonly scores = WELLBEING_SCORES;

  /** The check-in an adjacent tap would amend. */
  private pending: { key: string; score: number; at: number } | null = null;
  /** Clears `justLogged` when the amend window lapses. */
  private lapse: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      if (this.lapse !== null) clearTimeout(this.lapse);
    });
  }

  private armLapse(): void {
    if (this.lapse !== null) clearTimeout(this.lapse);
    this.lapse = setTimeout(() => this.justLogged.set(null), AMEND_WINDOW_MS);
  }

  async log(score: number): Promise<void> {
    const recent = this.pending;
    const armed = recent && Date.now() - recent.at < AMEND_WINDOW_MS;
    // The same face again is a double tap.
    if (armed && recent.score === score) return;
    if (armed && Math.abs(recent.score - score) === 1) {
      const tenths = midpoint(recent.score, score);
      await this.store.patch(recent.key, { scoreTenths: tenths });
      this.pending = null;
      this.justLogged.set(recent.key);
      this.armLapse();
      this.logged.emit();
      const key = recent.key;
      this.feedback.undo(`Logged ${scoreMeta(tenths).label}`, () => {
        this.justLogged.set(null);
        void this.store.remove(key);
      });
      return;
    }
    const key = await this.store.add({
      recordedAt: new Date().toISOString(),
      scoreTenths: toTenths(score),
      note: null,
    });
    this.pending = { key, score, at: Date.now() };
    this.justLogged.set(key);
    this.armLapse();
    this.logged.emit();
    this.feedback.undo(`Logged ${scoreMeta(toTenths(score)).label}`, () => {
      this.pending = null;
      this.justLogged.set(null);
      void this.store.remove(key);
    });
  }
}
