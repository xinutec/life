import { Injectable } from '@angular/core';

/**
 * The Android wrapper's origin-scoped reminder port (absent in a browser);
 * it can post any notification with any deep link. Fire-and-forget.
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
 * Device-local Android notifications through the native ReminderBridge, which
 * fires at a wall-clock time even with the app closed. Only inside the Android
 * app: `available` is false in a browser, and every method is then a no-op.
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
