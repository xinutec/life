import { Injectable } from '@angular/core';

/** The native interface injected by the Android wrapper (absent in a browser).
 *  `whenMs` is epoch milliseconds; scheduling the same `id` replaces its pending
 *  reminder. All methods are fire-and-forget. */
interface ReminderBridge {
  available(): boolean;
  schedule(id: string, whenMs: number, title: string, body: string, url: string): void;
  cancel(id: string): void;
}

interface ReminderWindow extends Window {
  ReminderBridge?: ReminderBridge;
}

/**
 * Schedules device-local Android notifications through the native ReminderBridge.
 * The bridge fires an alarm at a wall-clock time even when the app is closed —
 * only possible inside the Life Android app, so `available` is false in a plain
 * browser and callers must feature-detect before offering reminder UI. Every method
 * is a safe no-op when the bridge is absent, so callers needn't guard each call.
 */
@Injectable({ providedIn: 'root' })
export class Reminders {
  private readonly bridge = (window as ReminderWindow).ReminderBridge;

  /** True only inside the Android app (the native bridge is present). */
  get available(): boolean {
    try {
      return !!this.bridge?.available();
    } catch {
      return false;
    }
  }

  /** Schedule (or replace) reminder `id` to fire at `whenMs` (epoch ms). Tapping the
   *  notification opens the app at `url` (an in-app path, e.g. '/today'). */
  schedule(id: string, whenMs: number, title: string, body: string, url: string): void {
    try {
      this.bridge?.schedule(id, whenMs, title, body, url);
    } catch {
      /* bridge vanished mid-call — nothing to do */
    }
  }

  /** Cancel a pending reminder and dismiss any notification it already posted. */
  cancel(id: string): void {
    try {
      this.bridge?.cancel(id);
    } catch {
      /* no-op */
    }
  }
}
