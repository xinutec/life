import { Injectable } from '@angular/core';

/** The Android wrapper's reminder port; absent in a browser. */
interface ReminderBridge {
  postMessage(message: string): void;
}

interface ReminderWindow extends Window {
  ReminderBridge?: ReminderBridge;
}

/** Scheduling an `id` again replaces its pending reminder. */
type ReminderRequest =
  | { op: 'schedule'; id: string; whenMs: number; title: string; body: string; url: string }
  | { op: 'cancel'; id: string };

/** Android notifications that fire with the app closed; no-ops in a browser. */
@Injectable({ providedIn: 'root' })
export class Reminders {
  private readonly bridge = (window as ReminderWindow).ReminderBridge;

  /** An older app's port lacks `postMessage` and reads as absent. */
  get available(): boolean {
    return typeof this.bridge?.postMessage === 'function';
  }

  /** Tapping it opens the app at `url`. */
  schedule(id: string, whenMs: number, title: string, body: string, url: string): void {
    this.send({ op: 'schedule', id, whenMs, title, body, url });
  }

  /** Also dismisses a notification it already posted. */
  cancel(id: string): void {
    this.send({ op: 'cancel', id });
  }

  private send(request: ReminderRequest): void {
    if (!this.available) return;
    try {
      this.bridge?.postMessage(JSON.stringify(request));
    } catch {
      /* the bridge went away mid-call */
    }
  }
}
