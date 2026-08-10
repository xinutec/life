import { Component, DestroyRef, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';

import { BUILD_INFO } from '../../build-info';
import { LifeApi } from '../../life-api';
import { ConnectionStatus } from '../../models';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import {
  createRule,
  WellbeingReminder,
  WellbeingReminderRule,
} from '../../shared/wellbeing-reminder';
import { SwUpdates } from '../../sw-updates';

/** Settings — the natural home for app-level bits (the build version today; NC
 *  link, preferences, etc. later). The version is stamped into the bundle at
 *  build time (see scripts/stamp-version.mjs), so what shows here is the build
 *  actually running in *this* tab — a stale PWA reveals its own old sha rather
 *  than the server's current one. "Check for updates" forces the service worker
 *  to fetch a newer build and reload. */
@Component({
  selector: 'app-settings',
  templateUrl: './settings.html',
  styleUrl: './settings.scss',
  imports: [
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatListModule,
    MatFormFieldModule,
    MatInputModule,
  ],
})
export class Settings {
  private api = inject(LifeApi);
  private swUpdates = inject(SwUpdates);
  private feedback = inject(Feedback);
  private wellbeingReminder = inject(WellbeingReminder);

  protected readonly build = BUILD_INFO;
  /** Localized build time, or '' when unknown (a bare/dev stamp). */
  protected readonly builtAt = BUILD_INFO.builtAt
    ? new Date(BUILD_INFO.builtAt).toLocaleString()
    : '';
  protected readonly checking = signal(false);

  // The Nextcloud calendar link (app password, Login Flow v2). Separate from
  // signing in: identity OAuth cannot reach the DAV endpoints, so the calendar
  // needs its own long-lived credential (docs/design/overview.md §2b). The
  // backend has served this flow since before there was a button for it, which
  // is why nothing was ever linked.
  protected readonly ncStatus = signal<ConnectionStatus | null>(null);
  protected readonly ncBusy = signal(false);
  /** The approval URL, kept so it stays tappable if the popup was blocked —
   *  a WebView or a strict browser can refuse `window.open` even from a click,
   *  and losing the URL would strand the flow with no way back to it. */
  protected readonly ncUrl = signal<string | null>(null);

  constructor() {
    // Read the link's state on arrival rather than assuming "not connected":
    // the card's whole job is to say which it is, and defaulting to the wrong
    // one would invite a re-link that replaces a working credential.
    this.readNcStatus();
    // And again whenever this page comes back to the front.
    //
    // Approving the grant happens on NEXTCLOUD'S page, which on a phone takes
    // over the foreground — so the app that started the flow is backgrounded or
    // replaced while the only interesting thing happens elsewhere. A timer
    // running in that app is worthless: on 2026-08-10 the credential landed
    // server-side and the card sat on "Waiting for approval" indefinitely,
    // having made exactly ONE status request, the one on page load. Coming back
    // is the event worth listening to, and it fires whether the page was
    // backgrounded, replaced, or never left at all.
    const onReturn = (): void => {
      if (document.visibilityState === 'visible') this.readNcStatus();
    };
    document.addEventListener('visibilitychange', onReturn);
    window.addEventListener('focus', onReturn);
    inject(DestroyRef).onDestroy(() => {
      document.removeEventListener('visibilitychange', onReturn);
      window.removeEventListener('focus', onReturn);
    });
  }

  /** Ask the server where the link stands, and say so.
   *
   *  Never sets `not_linked` over a `null`-on-failure: unknown and unlinked are
   *  different answers and the card renders them differently. */
  private readNcStatus(): void {
    this.api.nextcloudStatus().subscribe({
      next: ({ status }) => {
        const was = this.ncStatus();
        this.ncStatus.set(status);
        // Only announce a link the user is waiting on. Saying "connected" on
        // every return to the tab would be noise about something that has not
        // changed.
        if (status === 'active' && was !== 'active' && this.ncBusy()) {
          this.feedback.notify('Nextcloud calendar connected.');
        }
        if (status === 'active') {
          this.ncBusy.set(false);
          this.ncUrl.set(null);
          return;
        }
        // Back here, still unlinked: the grant was abandoned, or is not
        // finished. Re-enable the button either way — leaving it disabled on a
        // "waiting" that nothing will ever end is a dead end with no way out of
        // it. The URL stays on screen, so carrying on is still one tap.
        this.ncBusy.set(false);
      },
      // Unknown stays unknown — see the template: it offers no button rather
      // than guessing, because both guesses are actionable and wrong.
      error: () => this.ncStatus.set(null),
    });
  }

  // Daily wellbeing-check-in reminders (device-local Android notifications). Each
  // rule is a time + a quiet window ("remind at 9am if I haven't checked in for 3
  // hours"); add as many as you like. The editor always shows so the config is
  // editable, but the reminders only fire inside the Life Android app.
  protected readonly reminderAvailable = this.wellbeingReminder.available;
  protected readonly rules = signal<WellbeingReminderRule[]>(
    this.wellbeingReminder.getConfig().rules,
  );

  /** Ask where to approve the grant, and send the user there.
   *
   *  Nothing is watched from here. The backend polls Nextcloud itself and
   *  stores the password when granted; this side finds out by asking again
   *  when the page comes back (see the constructor), which is the only moment
   *  it is reliably running.
   *
   *  `noopener` costs the ability to close that page for you — `window.open`
   *  returns null with it, so there is no handle — and that is the trade taken
   *  deliberately: a page that can reach back into this one through
   *  `window.opener` is worse than one you close yourself. Nextcloud's own
   *  "you can close this window" is it saying the same thing. */
  protected connectNextcloud(): void {
    if (this.ncBusy()) return;
    this.ncBusy.set(true);
    this.api.nextcloudConnect().subscribe({
      next: ({ login_url }) => {
        this.ncUrl.set(login_url);
        window.open(login_url, '_blank', 'noopener');
      },
      error: (e: unknown) => {
        this.ncBusy.set(false);
        this.feedback.error(`Could not reach Nextcloud${onlineHint(e)}`);
      },
    });
  }

  protected addRule(): void {
    this.rules.update((rs) => [...rs, createRule()]);
    this.saveRules();
  }

  protected removeRule(id: string): void {
    this.rules.update((rs) => rs.filter((r) => r.id !== id));
    this.saveRules();
  }

  protected setRuleTime(id: string, time: string): void {
    if (!time) return; // the picker was cleared — keep the last valid time
    this.rules.update((rs) => rs.map((r) => (r.id === id ? { ...r, time } : r)));
    this.saveRules();
  }

  protected setRuleQuietHours(id: string, hours: number): void {
    if (!(hours >= 1)) return; // ignore an empty/invalid entry
    this.rules.update((rs) => rs.map((r) => (r.id === id ? { ...r, quietHours: hours } : r)));
    this.saveRules();
  }

  private saveRules(): void {
    this.wellbeingReminder.setConfig({ rules: this.rules() });
  }

  protected async checkForUpdates(): Promise<void> {
    this.checking.set(true);
    try {
      const result = await this.swUpdates.checkNow();
      if (result === 'updating') {
        this.feedback.notify('New version found — updating…');
      } else if (result === 'current') {
        this.feedback.notify('You’re on the latest version.');
      } else if (result === 'failed') {
        // Covers both halves of checkNow: the check itself failing (offline, the
        // usual cause) and a staged build refusing to activate. Says neither.
        this.feedback.error('Couldn’t update — try again.');
      } else {
        this.feedback.error('Updates aren’t available in this build.');
      }
    } finally {
      this.checking.set(false);
    }
  }
}
