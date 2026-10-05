import { Component, DestroyRef, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';
import { scaffoldTitle } from '@xinutec/ui-scaffold';

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

/** The build running in this tab, the Nextcloud link and reminders. */
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
  protected readonly builtAt = BUILD_INFO.builtAt
    ? new Date(BUILD_INFO.builtAt).toLocaleString()
    : '';
  protected readonly checking = signal(false);

  // The calendar needs its own app password: login OAuth cannot reach DAV.
  protected readonly ncStatus = signal<ConnectionStatus | null>(null);
  protected readonly ncBusy = signal(false);
  /** Kept on screen in case `window.open` was blocked. */
  protected readonly ncUrl = signal<string | null>(null);

  constructor() {
    scaffoldTitle(() => 'Settings');
    // Read, never assumed: a wrong "not connected" invites replacing a working link.
    this.readNcStatus();
    // Approval happens in Nextcloud's page, so re-check on coming back.
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

  /** A failure leaves the status unknown, never `not_linked`. */
  private readNcStatus(): void {
    this.api.nextcloudStatus().subscribe({
      next: ({ status }) => {
        const was = this.ncStatus();
        this.ncStatus.set(status);
        // Announce only a link someone is waiting on.
        if (status === 'active' && was !== 'active' && this.ncBusy()) {
          this.feedback.notify('Nextcloud calendar connected.');
        }
        if (status === 'active') {
          this.ncBusy.set(false);
          this.ncUrl.set(null);
          return;
        }
        // Still unlinked: re-enable the button, or the wait never ends.
        this.ncBusy.set(false);
      },
      error: () => this.ncStatus.set(null),
    });
  }

  // Shown everywhere, fires only in the Android app.
  protected readonly reminderAvailable = this.wellbeingReminder.available;
  protected readonly rules = signal<WellbeingReminderRule[]>(
    this.wellbeingReminder.getConfig().rules,
  );

  /** The backend polls for the password; the page re-checks on return. */
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
        this.feedback.error('Couldn’t update — try again.');
      } else {
        this.feedback.error('Updates aren’t available in this build.');
      }
    } finally {
      this.checking.set(false);
    }
  }
}
