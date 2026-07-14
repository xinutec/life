import { Component, inject, output } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

import { Feedback } from './feedback';
import { WellbeingStore } from '../sync/wellbeing-store';

/** Readings are stored in TENTHS of a point (10..50): a 35 is a 3.5, the mood
 *  between two faces. Only half-points are recordable today, but the scale — and
 *  everything derived from it here — is arithmetic, not a lookup, so finer steps
 *  would need no changes below the check-in strip itself. */
export const TENTHS_PER_POINT = 10;
export const HALF_STEP_TENTHS = 5;

/** 35 → 3.5. The one place the scale is undone, so nothing else divides by ten. */
export function toPoints(tenths: number): number {
  return tenths / TENTHS_PER_POINT;
}

/** 4 → 40. Whole points only (the five faces); half-steps come from `midpoint`. */
export function toTenths(points: number): number {
  return points * TENTHS_PER_POINT;
}

/** True when the reading sits between two faces — a 3.5, not a 3 or a 4. */
export function isHalfStep(tenths: number): boolean {
  return tenths % TENTHS_PER_POINT !== 0;
}

/** The reading between two whole faces: 3 and 4 → 35 tenths. */
export function midpoint(a: number, b: number): number {
  return (toTenths(a) + toTenths(b)) / 2;
}

/** The whole faces a reading lights up: one for a 4, both neighbours for a 3.5.
 *  Two lit neighbours is how a half-step shows itself on a strip of whole faces. */
export function facesOf(tenths: number): number[] {
  const points = toPoints(tenths);
  return isHalfStep(tenths) ? [Math.floor(points), Math.ceil(points)] : [points];
}

/** Tapping `face` when `now` is the current reading, on a strip where one face
 *  means a whole and two adjacent faces mean the half between them:
 *
 *  - nothing set, or a face two or more away → that face, plainly.
 *  - the face NEXT to a whole → the half-step between the two (both light up).
 *  - either face OF a half-step → collapse to that whole (the other goes dark).
 *
 *  Null when the tap is a no-op (the lone face that's already on): the callers
 *  differ on what that means — the mood is required so it stays put, the optional
 *  energy clears — so this refuses to guess. */
export function nextReading(now: number | null | undefined, face: number): number | null {
  // From a half-step, ANY tap resolves to that whole face — whether it's one of
  // the two lit ones (collapse to it) or a distant one (just go there).
  if (now == null || isHalfStep(now)) return toTenths(face);
  const current = toPoints(now);
  if (current === face) return null; // already on, alone — the caller decides
  return Math.abs(current - face) === 1 ? midpoint(current, face) : toTenths(face);
}

/** The five wellbeing levels: score, label + Material face icon. Shared so the
 *  check-in strip, the history timeline and the edit sheet all agree. */
export const WELLBEING_SCORES: readonly { score: number; label: string; icon: string }[] = [
  { score: 1, label: 'awful', icon: 'sentiment_very_dissatisfied' },
  { score: 2, label: 'low', icon: 'sentiment_dissatisfied' },
  { score: 3, label: 'okay', icon: 'sentiment_neutral' },
  { score: 4, label: 'good', icon: 'sentiment_satisfied' },
  { score: 5, label: 'great', icon: 'sentiment_very_satisfied' },
];

/** The five energy levels: value (1..5, higher = better — like mood), label +
 *  battery icon. Ordered ascending (drained → energetic) so the battery fills as
 *  energy rises, mirroring the mood faces left-to-right. */
export const ENERGY_LEVELS: readonly { energy: number; label: string; icon: string }[] = [
  { energy: 1, label: 'drained', icon: 'battery_alert' },
  { energy: 2, label: 'low', icon: 'battery_2_bar' },
  { energy: 3, label: 'okay', icon: 'battery_3_bar' },
  { energy: 4, label: 'good', icon: 'battery_5_bar' },
  { energy: 5, label: 'energetic', icon: 'battery_full' },
];

/** What one reading looks like: the word for it, the icon, and the colour. */
export interface LevelMeta {
  label: string;
  icon: string;
  /** A CSS colour: one rung of the ramp, or — for a half-step — the blend of the
   *  two it sits between, so the dot's colour says the same thing as its height. */
  color: string;
}

/** Resolve a reading in tenths against a five-rung scale.
 *
 *  A half-step takes the LOWER rung's icon and both rungs' words ("okay–good").
 *  The icon is the level you definitely reached — a 3.5 is an okay that was
 *  heading for good, not a good that fell short — while the label and the colour
 *  carry the half. Rounding the icon up would quietly promote every half-step. */
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

/** The mood level (label, face icon, ramp colour) for a reading in tenths. */
export function scoreMeta(tenths: number): LevelMeta {
  return levelMeta(tenths, WELLBEING_SCORES, 'wb-score');
}

/** The energy level (label, battery icon, ramp colour) for a reading in tenths. */
export function energyMeta(tenths: number): LevelMeta {
  return levelMeta(tenths, ENERGY_LEVELS, 'wb-score');
}

/** How long after a tap its neighbour still counts as "the same check-in".
 *
 *  A minute, not the six seconds the Undo snackbar lives for. Tying it to the
 *  snackbar made the window visible, which is tidy but is a fact about the UI, not
 *  about how long "4… no, a bit below that" takes to think. The only thing a long
 *  window can get wrong is swallowing a genuine SECOND check-in on an adjacent
 *  face — which needs the mood to move by exactly one face, and to be recorded
 *  twice, inside a minute. The amend announces itself when it fires and is one Undo
 *  away; a hedge lost to a stopwatch is just gone. */
const AMEND_WINDOW_MS = 60_000;

/** The one-tap mood check-in: five face buttons that log an entry at "now".
 *  Logging is instant and offline; an Undo snackbar covers a mis-tap. Tapping a
 *  neighbouring face while that snackbar is up amends the entry to the half-step
 *  between the two ("4, but a bit lower"). Note and time adjustments are follow-ups
 *  on the history screen, never prerequisites. */
@Component({
  selector: 'app-wellbeing-checkin',
  templateUrl: './wellbeing-checkin.html',
  styleUrl: './wellbeing-checkin.scss',
  imports: [MatButtonModule, MatIconModule],
})

export class WellbeingCheckin {
  private store = inject(WellbeingStore);
  private feedback = inject(Feedback);

  /** Emitted after a check-in is logged (so a host can e.g. scroll to it). */
  readonly logged = output<void>();

  readonly scores = WELLBEING_SCORES;

  /** The check-in this tap might still be amending: tap Good then Okay and you
   *  meant one 3.5, not a 4 and then a 3. Only an ADJACENT face amends — two faces
   *  apart isn't a half-step, it's a correction (or a different feeling), and both
   *  of those are better served by leaving the first entry alone. */
  private pending: { key: string; score: number; at: number } | null = null;

  async log(score: number): Promise<void> {
    const recent = this.pending;
    const armed = recent && Date.now() - recent.at < AMEND_WINDOW_MS;
    // The same face again is a stray double tap, not a second check-in a minute
    // apart on the identical score. Swallow it: two identical entries is never what
    // the second tap meant, and the first one is already logged and undoable.
    if (armed && recent.score === score) return;
    if (armed && Math.abs(recent.score - score) === 1) {
      const tenths = midpoint(recent.score, score);
      await this.store.patch(recent.key, { scoreTenths: tenths });
      this.pending = null;
      this.logged.emit();
      const key = recent.key;
      this.feedback.undo(`Logged ${scoreMeta(tenths).label}`, () => void this.store.remove(key));
      return;
    }
    // Log at "now" immediately (offline-ok); a mis-tap is one Undo away. A
    // just-created entry is removed outright on undo — no server restore needed.
    const key = await this.store.add({
      recordedAt: new Date().toISOString(),
      scoreTenths: toTenths(score),
      note: null,
    });
    this.pending = { key, score, at: Date.now() };
    this.logged.emit();
    this.feedback.undo(`Logged ${scoreMeta(toTenths(score)).label}`, () => {
      this.pending = null;
      void this.store.remove(key);
    });
  }
}
