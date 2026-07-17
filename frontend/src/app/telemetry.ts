import { DOCUMENT, Injectable, inject } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import { filter } from 'rxjs';

import { LifeApi } from './life-api';
import { TelemetryEvent } from './models';

/** The verbatim label of the nearest interactive ancestor of `node`, or null if
 *  the tap didn't land on (or inside) a control. Reuses the accessible name the
 *  user already sees — aria-label first, then trimmed text, then a title — so
 *  nothing needs a bespoke tracking attribute. Exported for its own test. */
export function labelFor(node: EventTarget | null): string | null {
  if (!(node instanceof Element)) return null;
  const el = node.closest(
    'button, a, [role="button"], [role="tab"], [role="menuitem"], [role="switch"], input[type="submit"]',
  );
  if (!el) return null;
  const aria = el.getAttribute('aria-label')?.trim();
  if (aria) return aria;
  // Read the visible label minus decorative bits. A Material icon renders its
  // ligature NAME as text ("store"), which would otherwise prefix every
  // icon+label button ("storeFind at Asda"); aria-hidden content is by
  // definition not part of what the control says. Strip both on a clone so the
  // live DOM is untouched.
  const clone = el.cloneNode(true) as Element;
  clone.querySelectorAll('mat-icon, [aria-hidden="true"]').forEach((n) => n.remove());
  const text = (clone.textContent ?? '').replace(/\s+/g, ' ').trim();
  if (text) return text;
  const title = el.getAttribute('title')?.trim();
  if (title) return title;
  return null;
}

/**
 * Client activity trace. Captures every route change (Router events) and every
 * tap on a control (one global click listener), batches them, and POSTs them to
 * /api/telemetry, where they fold into the backend log stream alongside the API
 * requests they cause. Instrumented ONCE, here — no screen knows it exists.
 *
 * Best-effort by design: a failed send is dropped, never retried, never
 * surfaced. Telemetry must never get in the way of the app it observes.
 */
@Injectable({ providedIn: 'root' })
export class Telemetry {
  private readonly api = inject(LifeApi);
  private readonly router = inject(Router);
  private readonly doc = inject(DOCUMENT);

  private queue: TelemetryEvent[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;

  /** Flush cadence, and a queue cap that forces an early flush so a burst of
   *  activity can't grow the buffer without bound between ticks. */
  private static readonly FLUSH_MS = 5000;
  private static readonly MAX_QUEUE = 50;

  /** Wire the two capture points. Called once from the app shell; idempotent. */
  init(): void {
    if (this.timer !== null) return;

    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.enqueue('nav', e.urlAfterRedirects, null));

    // Capture phase: we still see the tap even if a handler stops propagation.
    this.doc.addEventListener(
      'click',
      (ev) => {
        const label = labelFor(ev.target);
        if (label !== null) this.enqueue('tap', this.router.url, label);
      },
      { capture: true },
    );

    this.timer = setInterval(() => this.flush(false), Telemetry.FLUSH_MS);

    // A best-effort final flush when the app is backgrounded (the WebView may
    // freeze us — see the cached-app freezer note) or closed, so the last few
    // events aren't stranded in the queue.
    this.doc.addEventListener('visibilitychange', () => {
      if (this.doc.visibilityState === 'hidden') this.flush(true);
    });
  }

  private enqueue(kind: string, path: string, label: string | null): void {
    this.queue.push({ kind, path, label, at: Date.now() });
    if (this.queue.length >= Telemetry.MAX_QUEUE) this.flush(false);
  }

  private flush(final: boolean): void {
    if (this.queue.length === 0) return;
    const batch = this.queue;
    this.queue = [];
    // On backgrounding, sendBeacon survives a freeze an in-flight fetch would
    // not; otherwise go through the normal API so the session cookie rides along.
    if (final && this.doc.defaultView?.navigator.sendBeacon) {
      this.doc.defaultView.navigator.sendBeacon(
        '/api/telemetry',
        new Blob([JSON.stringify(batch)], { type: 'application/json' }),
      );
      return;
    }
    this.api.sendTelemetry(batch).subscribe({ error: () => {} });
  }
}
