import { Injectable, inject } from '@angular/core';
import { SwUpdate, VersionReadyEvent } from '@angular/service-worker';
import { filter } from 'rxjs';

/** Updates arriving this soon after start() reload immediately — nothing is in
 *  progress yet, so you basically never see it. */
const STARTUP_MS = 10_000;

/** Marks that we have already auto-reloaded out of an unrecoverable service
 *  worker state. Session-scoped so it survives that very reload: a build broken
 *  badly enough to wedge again would otherwise loop the app forever, so we spend
 *  exactly one automatic recovery per tab and then leave it alone. */
const RECOVERY_KEY = 'life.sw-recovery-attempted';

/** What we are currently doing about updates.
 *
 *  - `idle`   — nothing found, nothing waiting.
 *  - `staged` — a newer build is downloaded, waiting for a safe moment.
 *  - `asked`  — a manual check is in flight; its result applies immediately.
 *
 *  One field rather than a pair of booleans, because the pair could express a
 *  state the policy has no answer for: "the user asked" was reset on a single
 *  branch, so a *failed* check latched it on for the rest of the session and
 *  every later background update then reloaded mid-use — exactly what the
 *  deferral below exists to prevent. */
type Mode = 'idle' | 'staged' | 'asked';

/** The outcome of a manual "Check for updates". */
export type UpdateOutcome =
  /** A newer build is being activated — the page is about to reload. */
  | 'updating'
  /** Already on the latest build. */
  | 'current'
  /** The check (or the activation) failed — nothing has changed. */
  | 'failed'
  /** No service worker, so there is nothing to check (dev build). */
  | 'unsupported';

/** Self-update: when the service worker has finished caching a newer version,
 *  activate it and reload — but never mid-use. The rules:
 *
 *  - **Startup / hidden**: reload right away (invisible either way).
 *  - **Mid-session, visible**: defer the reload until the app is next
 *    backgrounded, so an update never eats a half-typed form. Combined with the
 *    visibility re-check below, a PWA left open for days updates itself the
 *    moment you switch away and is fresh when you come back.
 *  - **Becoming visible**: re-check for a newer build. ngsw only re-checks on
 *    its own at a navigation, which a resumed long-lived tab never performs —
 *    this is the fix for the stale-tab problem.
 *
 *  No reload loop: after the reload the new version is the active one, so no
 *  further VERSION_READY fires. */
@Injectable({ providedIn: 'root' })
export class SwUpdates {
  private readonly sw = inject(SwUpdate);
  private startedAt = 0;
  private mode: Mode = 'idle';

  start(): void {
    if (!this.sw.isEnabled) return; // dev build has no service worker
    this.startedAt = Date.now();
    this.sw.versionUpdates
      .pipe(filter((e): e is VersionReadyEvent => e.type === 'VERSION_READY'))
      .subscribe(() => this.onVersionReady());
    // The cached build is broken and the server no longer holds the files to
    // repair it — which is what a roll-forward deploy of :latest leaves behind
    // for a client whose cache was evicted meanwhile. Nothing recovers from here
    // except a fresh load, so take one.
    this.sw.unrecoverable.subscribe(() => this.recover());
    document.addEventListener('visibilitychange', () => this.onVisibilityChange());
    this.backgroundCheck();
  }

  private onVersionReady(): void {
    const inStartup = Date.now() - this.startedAt < STARTUP_MS;
    if (this.mode === 'asked' || inStartup || document.visibilityState === 'hidden') {
      void this.applyUpdate();
    } else {
      this.mode = 'staged';
    }
  }

  private onVisibilityChange(): void {
    if (document.visibilityState === 'hidden') {
      if (this.mode === 'staged') void this.applyUpdate();
    } else {
      this.backgroundCheck();
    }
  }

  /** A check nobody asked for (startup, or returning to a stale tab). Failure is
   *  ordinary here — being offline is the common case — so it goes unreported;
   *  the next time the app becomes visible it simply tries again. */
  private backgroundCheck(): void {
    void this.sw.checkForUpdate().catch(() => undefined);
  }

  /** Activate the staged build and reload. Resolves false when activation
   *  failed, so a caller never claims an update is happening that isn't. */
  async applyUpdate(): Promise<boolean> {
    try {
      // Resolves false when there was nothing to activate; reload regardless,
      // since a fresh load is the reliable way to land on the current build.
      await this.sw.activateUpdate();
    } catch {
      // Keep the build staged so the next backgrounding — or the next manual
      // check — retries it, rather than dropping it and going quietly stale.
      this.mode = 'staged';
      return false;
    }
    this.reload();
    return true;
  }

  /** The one place the page is thrown away. Its own method so tests can assert
   *  "this would have reloaded" without navigating the test runner. */
  reload(): void {
    document.location.reload();
  }

  private recover(): void {
    if (sessionStorage.getItem(RECOVERY_KEY)) return; // already spent this tab's attempt
    sessionStorage.setItem(RECOVERY_KEY, '1');
    this.reload();
  }

  /** Manual "Check for updates" (Settings). Never rejects — every failure comes
   *  back as `'failed'` so the caller can say so. */
  async checkNow(): Promise<UpdateOutcome> {
    if (!this.sw.isEnabled) return 'unsupported';
    // A newer build may already be downloaded and STAGED — VERSION_READY fired
    // mid-session and we held the reload so it wouldn't interrupt. In that case
    // checkForUpdate() reports "nothing newer" (false), and we'd wrongly say
    // "you're on the latest" while a staged update sits waiting. The user just
    // asked, so activate that pending update now.
    if (this.mode === 'staged') {
      return (await this.applyUpdate()) ? 'updating' : 'failed';
    }
    // Otherwise ask the SW to look: checkForUpdate() resolves true when a new
    // version was discovered; the VERSION_READY subscription then activates it.
    const previous = this.mode;
    this.mode = 'asked';
    let found: boolean;
    try {
      found = await this.sw.checkForUpdate();
    } catch {
      this.disarm(previous); // the check failed — nothing is coming to apply
      return 'failed';
    }
    if (!found) {
      this.disarm(previous);
      return 'current';
    }
    // Stay 'asked'. The VERSION_READY this promises can land after we return,
    // and the user asked for it, so it must apply at once instead of deferring.
    // applyUpdate() takes the state back off 'asked' if activation fails.
    return 'updating';
  }

  /** Take the state back off 'asked' once nothing is coming. Left latched it
   *  would make the next *background* update reload mid-use. Skipped when
   *  applyUpdate() has meanwhile staged a build — that is the newer news. */
  private disarm(previous: Mode): void {
    if (this.mode === 'asked') this.mode = previous;
  }
}
