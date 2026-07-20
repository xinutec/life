import { Component, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';

import { BUILD_INFO } from '../../build-info';
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
  private swUpdates = inject(SwUpdates);
  private feedback = inject(Feedback);
  private wellbeingReminder = inject(WellbeingReminder);

  protected readonly build = BUILD_INFO;
  /** Localized build time, or '' when unknown (a bare/dev stamp). */
  protected readonly builtAt = BUILD_INFO.builtAt
    ? new Date(BUILD_INFO.builtAt).toLocaleString()
    : '';
  protected readonly checking = signal(false);

  // Daily wellbeing-check-in reminders (device-local Android notifications). Each
  // rule is a time + a quiet window ("remind at 9am if I haven't checked in for 3
  // hours"); add as many as you like. The editor always shows so the config is
  // editable, but the reminders only fire inside the Life Android app.
  protected readonly reminderAvailable = this.wellbeingReminder.available;
  protected readonly rules = signal<WellbeingReminderRule[]>(
    this.wellbeingReminder.getConfig().rules,
  );

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
      } else {
        this.feedback.error('Updates aren’t available in this build.');
      }
    } finally {
      this.checking.set(false);
    }
  }
}
