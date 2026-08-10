import { Component, inject, signal } from '@angular/core';
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

/** How often to ask whether the grant has landed, and how many times. The
 *  backend gives up after five minutes; matching that means the watcher dies
 *  with the flow it is watching rather than outliving it. */
const NC_POLL_MS = 3000;
const NC_POLLS = 100;

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
    this.api.nextcloudStatus().subscribe({
      next: ({ status }) => this.ncStatus.set(status),
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

  /** Ask where to approve the grant, open it, and watch for it landing.
   *
   *  The backend polls Nextcloud itself and stores the password when granted,
   *  so this only has to notice that it happened. Bounded: the server gives up
   *  after five minutes, and a watcher that outlived it would spin forever
   *  against a flow nobody is going to complete. */
  protected connectNextcloud(): void {
    if (this.ncBusy()) return;
    this.ncBusy.set(true);
    this.api.nextcloudConnect().subscribe({
      next: ({ login_url }) => {
        this.ncUrl.set(login_url);
        window.open(login_url, '_blank', 'noopener');
        this.watchForLink();
      },
      error: (e: unknown) => {
        this.ncBusy.set(false);
        this.feedback.error(`Could not reach Nextcloud${onlineHint(e)}`);
      },
    });
  }

  private watchForLink(): void {
    let left = NC_POLLS;
    const timer = setInterval(() => {
      if (left-- <= 0) {
        clearInterval(timer);
        this.ncBusy.set(false);
        this.ncUrl.set(null);
        this.feedback.error('Nextcloud wasn’t approved in time — try again.');
        return;
      }
      this.api.nextcloudStatus().subscribe({
        next: ({ status }) => {
          if (status === 'not_linked') return;
          clearInterval(timer);
          this.ncStatus.set(status);
          this.ncBusy.set(false);
          this.ncUrl.set(null);
          this.feedback.notify('Nextcloud calendar connected.');
        },
        // A blip mid-poll is not a failure of the flow: keep watching, and let
        // the bounded count end it if the connection is really gone.
        error: () => {},
      });
    }, NC_POLL_MS);
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
