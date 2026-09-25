import { Injectable } from '@angular/core';

/**
 * The native port injected by the Android wrapper (absent in a browser).
 *
 * An origin-scoped message port (`WebViewCompat.addWebMessageListener`), so it
 * is never injected into a frame that isn't this app — it can schedule a
 * notification saying anything and deep-linking anywhere.
 *
 * Everything here is fire-and-forget, so nothing needs a reply.
 */
interface ReminderBridge {
  postMessage(message: string): void;
}

interface ReminderWindow extends Window {
  ReminderBridge?: ReminderBridge;
}

/** A reminder to schedule, or one to cancel. `whenMs` is epoch milliseconds;
 *  scheduling an `id` again replaces its pending reminder. */
type ReminderRequest =
  | { op: 'schedule'; id: string; whenMs: number; title: string; body: string; url: string }
  | { op: 'cancel'; id: string };

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

  /** True only inside the Android app: the port is injected only for this
   *  app's own origin. An older app injects a shape without `postMessage`,
   *  which reads as absent rather than throwing. */
  get available(): boolean {
    return typeof this.bridge?.postMessage === 'function';
  }

  /** Schedule (or replace) reminder `id` to fire at `whenMs` (epoch ms). Tapping the
   *  notification opens the app at `url` (an in-app path, e.g. '/today'). */
  schedule(id: string, whenMs: number, title: string, body: string, url: string): void {
    this.send({ op: 'schedule', id, whenMs, title, body, url });
  }

  /** Cancel a pending reminder and dismiss any notification it already posted. */
  cancel(id: string): void {
    this.send({ op: 'cancel', id });
  }

  private send(request: ReminderRequest): void {
    if (!this.available) return;
    try {
      this.bridge?.postMessage(JSON.stringify(request));
    } catch {
      /* bridge vanished mid-call — nothing to do */
    }
  }
}
