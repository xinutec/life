import { Injectable, inject } from '@angular/core';
import { take } from 'rxjs';

import { WellbeingStore } from '../sync/wellbeing-store';
import { Reminders } from './reminders';

/** One daily wellbeing-reminder rule: fire at local `time`, but only if there's been
 *  no check-in within the last `quietHours` — so "if I haven't logged anything for
 *  3 hours and it's 9am, remind me" is `{ time: '09:00', quietHours: 3 }`. The `id`
 *  is stable across edits so it keys the native alarm (re-scheduling replaces it). */
export interface WellbeingReminderRule {
  id: string;
  time: string;
  quietHours: number;
}

/** Device-local reminder config: an unordered set of daily rules. Held in
 *  localStorage (not synced) because the alarms fire on THIS phone — it's a
 *  per-device preference. An empty list means no reminders. */
export interface WellbeingReminderConfig {
  rules: WellbeingReminderRule[];
}

/** A tapped reminder lands on Today, which carries the one-tap check-in strip. */
const REMINDER_URL = '/today';
const REMINDER_TITLE = 'Wellbeing check-in';
const REMINDER_BODY = 'How are you feeling right now?';
const STORAGE_KEY = 'life.reminder.wellbeing';
const ARMED_KEY = 'life.reminder.wellbeing.armed';
const HOUR_MS = 3_600_000;

/** Parse an 'HH:MM' local time to [hours, minutes], or null if malformed. */
export function parseHhMm(time: string): [number, number] | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time.trim());
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  if (h < 0 || h > 23 || min < 0 || min > 59) return null;
  return [h, min];
}

/** A new rule with a freshly minted id (used by the settings editor). */
export function createRule(time = '09:00', quietHours = 3): WellbeingReminderRule {
  return { id: crypto.randomUUID(), time, quietHours };
}

/**
 * When a rule should next fire (epoch ms), or null if it can't (malformed time).
 * Fires at the next occurrence of the rule's local time that is both in the future
 * and at least `quietHours` after the last check-in. Because the only way to check
 * in is in the app — which re-arms — the last-check-in instant can't change between
 * arming and firing, so evaluating the quiet window at arm time gives the same
 * answer it would at fire time: no server round-trip, and a check-in simply pushes
 * the rule to the next day.
 */
export function nextFireForRule(
  rule: WellbeingReminderRule,
  now: Date,
  lastCheckinMs: number | null,
): number | null {
  const hm = parseHhMm(rule.time);
  if (!hm || !(rule.quietHours >= 0)) return null;
  const windowMs = rule.quietHours * HOUR_MS;
  const cand = new Date(now);
  cand.setHours(hm[0], hm[1], 0, 0);
  // Walk forward a day at a time to the first firing that's in the future AND past
  // the quiet window. The window is only ever unmet for the current day (the gap
  // grows by 24h each step), so this settles within one or two iterations.
  for (let i = 0; i < 8; i++) {
    const t = cand.getTime();
    const quietElapsed = lastCheckinMs === null || t - lastCheckinMs >= windowMs;
    if (t > now.getTime() && quietElapsed) return t;
    cand.setDate(cand.getDate() + 1);
  }
  return null;
}

/** The most recent check-in instant across all entries, or null if there are none. */
function lastCheckin(items: readonly { recordedAt: string }[]): number | null {
  let latest: number | null = null;
  for (const i of items) {
    const t = Date.parse(i.recordedAt);
    if (!Number.isNaN(t) && (latest === null || t > latest)) latest = t;
  }
  return latest;
}

function loadConfig(): WellbeingReminderConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { rules: [] };
    const parsed = JSON.parse(raw) as Partial<WellbeingReminderConfig>;
    const rules = Array.isArray(parsed.rules) ? parsed.rules : [];
    // Keep only well-formed rules; drop anything a hand-edit or old build left.
    const clean = rules.filter(
      (r): r is WellbeingReminderRule =>
        !!r &&
        typeof r.id === 'string' &&
        typeof r.time === 'string' &&
        parseHhMm(r.time) !== null &&
        typeof r.quietHours === 'number' &&
        r.quietHours >= 0,
    );
    return { rules: clean };
  } catch {
    return { rules: [] };
  }
}

function saveConfig(config: WellbeingReminderConfig): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    /* private mode / quota — the reminders just won't survive a reload */
  }
}

function loadArmedIds(): string[] {
  try {
    const raw = localStorage.getItem(ARMED_KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

function saveArmedIds(ids: string[]): void {
  try {
    localStorage.setItem(ARMED_KEY, JSON.stringify(ids));
  } catch {
    /* best effort */
  }
}

/**
 * Drives the daily wellbeing-check-in reminders off the native {@link Reminders}
 * bridge. Each rule is one wall-clock alarm, keyed by the rule's id; the whole set
 * is re-armed on every app open and every check-in change (the "simple" model — an
 * alarm survives the app closing but not a reboot, and is re-armed on the next
 * open). A no-op outside the Android app, where the bridge is absent.
 */
@Injectable({ providedIn: 'root' })
export class WellbeingReminder {
  private readonly reminders = inject(Reminders);
  private readonly store = inject(WellbeingStore);
  private cfg: WellbeingReminderConfig = loadConfig();
  // Rule ids we last armed, so a rule the user removed gets its alarm cancelled even
  // across a reload (localStorage-backed; reboots clear the alarms themselves).
  private armedIds = new Set<string>(loadArmedIds());

  /** Whether reminders can actually fire here (i.e. we're in the Android app). */
  get available(): boolean {
    return this.reminders.available;
  }

  getConfig(): WellbeingReminderConfig {
    return { rules: this.cfg.rules.map((r) => ({ ...r })) };
  }

  /** Persist a new rule set and re-arm immediately from the current check-ins. */
  setConfig(config: WellbeingReminderConfig): void {
    this.cfg = { rules: config.rules.map((r) => ({ ...r })) };
    saveConfig(this.cfg);
    this.store.items$.pipe(take(1)).subscribe((items) => this.rearm(items));
  }

  /** Subscribe to check-ins and re-arm on every change. The first emission is the
   *  app open; later ones are a check-in added or removed. Called once at startup. */
  init(): void {
    this.store.items$.subscribe((items) => this.rearm(items));
  }

  private rearm(items: readonly { recordedAt: string }[]): void {
    if (!this.reminders.available) return;
    const now = new Date();
    const last = lastCheckin(items);
    const wanted = new Set(this.cfg.rules.map((r) => r.id));
    // Cancel alarms for rules that no longer exist.
    for (const id of this.armedIds) {
      if (!wanted.has(id)) this.reminders.cancel(id);
    }
    const armed = new Set<string>();
    for (const rule of this.cfg.rules) {
      const at = nextFireForRule(rule, now, last);
      if (at === null) {
        this.reminders.cancel(rule.id);
      } else {
        this.reminders.schedule(rule.id, at, REMINDER_TITLE, REMINDER_BODY, REMINDER_URL);
        armed.add(rule.id);
      }
    }
    this.armedIds = armed;
    saveArmedIds([...armed]);
  }
}
