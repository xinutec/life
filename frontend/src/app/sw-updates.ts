import { Injectable, inject } from '@angular/core';
import { SwUpdate, VersionReadyEvent } from '@angular/service-worker';
import {
  type PagePort,
  type ServiceWorkerPort,
  SwUpdates as SwUpdatePolicy,
  type UpdateOutcome,
} from '@xinutec/ui-harness/sw-updates';
import { filter } from 'rxjs';

export type { UpdateOutcome };

/** Session-scoped, so it survives the one recovery reload. */
const RECOVERY_KEY = 'life.sw-recovery-attempted';

/** Angular wiring for the update policy in `@xinutec/ui-harness/sw-updates`. */
@Injectable({ providedIn: 'root' })
export class SwUpdates {
  private readonly sw = inject(SwUpdate);

  private readonly serviceWorker: ServiceWorkerPort = ((sw: SwUpdate) => ({
    // A getter over a local: `isEnabled` must be read live.
    get isEnabled(): boolean {
      return sw.isEnabled;
    },
    onVersionReady: (handler: () => void): void => {
      sw.versionUpdates
        .pipe(filter((event): event is VersionReadyEvent => event.type === 'VERSION_READY'))
        .subscribe(() => handler());
    },
    onUnrecoverable: (handler: () => void): void => {
      sw.unrecoverable.subscribe(() => handler());
    },
    checkForUpdate: () => sw.checkForUpdate(),
    activateUpdate: () => sw.activateUpdate(),
  }))(this.sw);

  private readonly page: PagePort = {
    get hidden(): boolean {
      return document.visibilityState === 'hidden';
    },
    onVisibilityChange: (handler: () => void): void => {
      document.addEventListener('visibilitychange', handler);
    },
    recoveryAttempted: () => sessionStorage.getItem(RECOVERY_KEY) !== null,
    markRecoveryAttempted: () => sessionStorage.setItem(RECOVERY_KEY, '1'),
    reload: () => this.reload(),
    now: () => Date.now(),
  };

  private readonly policy = new SwUpdatePolicy(this.serviceWorker, this.page);

  start(): void {
    this.policy.start();
  }

  /** Never rejects: a failure comes back as `'failed'`. */
  checkNow(): Promise<UpdateOutcome> {
    return this.policy.checkNow();
  }

  /** A method so tests can see it without reloading the runner. */
  reload(): void {
    document.location.reload();
  }
}
