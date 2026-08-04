import { Injectable } from '@angular/core';

/**
 * The native port injected by the Android wrapper (absent in a browser).
 *
 * A message port rather than the three plain methods it used to be. The wrapper
 * injects it with `WebViewCompat.addWebMessageListener`, whose origin rules keep
 * it out of any frame that isn't this app — the API it replaced was injected into
 * every frame in the WebView, including iframes, and this one could schedule a
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

  /** True only inside the Android app.
   *
   *  The port's presence *is* the answer now: the wrapper only injects it for
   *  this app's own origin, so there is nothing left for an `available()` call to
   *  establish that being able to ask hasn't already established. An app older
   *  than this page injects the previous shape, which has no `postMessage` — that
   *  reads as no bridge, and reminders are quietly unavailable until it's
   *  updated, rather than throwing on the first call. */
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
