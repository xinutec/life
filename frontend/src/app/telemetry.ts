import { DOCUMENT, Injectable, inject } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import { TelemetryCore } from '@xinutec/ui-harness/telemetry';
import { filter } from 'rxjs';

/**
 * The Angular binding for the fleet's activity trace, whose logic lives in
 * `@xinutec/ui-harness/telemetry`. The `@Injectable` can't ship from there: a
 * decorator compiled by plain `tsc` fails production builds ("JIT compiler
 * unavailable"). Instrumented once from the shell; screens need no annotation.
 */
@Injectable({ providedIn: 'root' })
export class Telemetry {
  private readonly router = inject(Router);
  private readonly doc = inject(DOCUMENT);
  private readonly core = new TelemetryCore(this.doc);

  /** Something the user met and nobody else saw: an uncaught error or a failed request. */
  failure(kind: 'error' | 'http', what: string): void {
    this.core.record(kind, this.router.url, what);
  }

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
