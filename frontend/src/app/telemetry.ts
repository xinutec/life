import { DOCUMENT, Injectable, inject } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import { TelemetryCore } from '@xinutec/ui-harness/telemetry';
import { filter } from 'rxjs';

/**
 * The Angular binding for the fleet's activity trace; the queue, flushing,
 * transport and label rules live in `@xinutec/ui-harness/telemetry`. The
 * `@Injectable` can't ship from there: that package is built by plain `tsc`,
 * and an uncompiled decorator fails production builds with "JIT compiler
 * unavailable".
 *
 * Instrumented once from the app shell, so screens need no annotation.
 */
@Injectable({ providedIn: 'root' })
export class Telemetry {
  private readonly router = inject(Router);
  private readonly doc = inject(DOCUMENT);
  private readonly core = new TelemetryCore(this.doc);

  /** Wire the two capture points. Called once from the app shell; idempotent. */
  init(): void {
    if (this.core.started) return;

    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.core.record('nav', e.urlAfterRedirects, null));

    // Capture phase, so the tap is seen even where a handler stops propagation.
    this.doc.addEventListener('click', (ev) => this.core.recordTap(ev.target, this.router.url), {
      capture: true,
    });

    this.core.start();
  }
}
