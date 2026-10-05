import { Injectable, inject } from '@angular/core';
import { take } from 'rxjs';

import { WellbeingStore } from '../sync/wellbeing-store';
import { Reminders } from './reminders';
import { isRecord } from './narrow';

/** Remind at local `time` unless there was a check-in in the last `quietHours`.
 *  `id` keys the native alarm. */
export interface WellbeingReminderRule {
  id: string;
  time: string;
  quietHours: number;
}

/** Per device, not synced: the alarms fire on this phone. */
export interface WellbeingReminderConfig {
  rules: WellbeingReminderRule[];
}

/** Today has the check-in strip. */
const REMINDER_URL = '/today';
const REMINDER_TITLE = 'Wellbeing check-in';
const REMINDER_BODY = 'How are you feeling right now?';
const STORAGE_KEY = 'life.reminder.wellbeing';
const ARMED_KEY = 'life.reminder.wellbeing.armed';
const HOUR_MS = 3_600_000;

export function parseHhMm(time: string): [number, number] | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  if (h > 23 || min > 59) return null;
  return [h, min];
}

export function createRule(time = '09:00', quietHours = 3): WellbeingReminderRule {
  return { id: crypto.randomUUID(), time, quietHours };
}

/** The next occurrence of the rule's time at least `quietHours` after the last
 *  check-in, or null for a malformed rule. */
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
  for (let i = 0; i < 8; i++) {
    const t = cand.getTime();
    const quietElapsed = lastCheckinMs === null || t - lastCheckinMs >= windowMs;
    if (t > now.getTime() && quietElapsed) return t;
    cand.setDate(cand.getDate() + 1);
  }
  return null;
}

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
    // The blob can be from any earlier build, so every rule is checked.
    const parsed: unknown = JSON.parse(raw);
    const rulesField = isRecord(parsed) ? parsed['rules'] : null;
    const rules: unknown[] = Array.isArray(rulesField) ? rulesField : [];
    const clean = rules.filter((r): r is WellbeingReminderRule => {
      if (!isRecord(r)) return false;
      const time = r['time'];
      return (
        typeof r['id'] === 'string' &&
        typeof time === 'string' &&
        parseHhMm(time) !== null &&
        typeof r['quietHours'] === 'number' &&
        r['quietHours'] >= 0
      );
    });
    return { rules: clean };
  } catch {
    return { rules: [] };
  }
}

function saveConfig(config: WellbeingReminderConfig): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    /* storage unavailable: the rules last until a reload */
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

/** One alarm per rule, re-armed on every open and check-in change: an alarm
 *  survives the app closing but not a reboot. */
@Injectable({ providedIn: 'root' })
export class WellbeingReminder {
  private readonly reminders = inject(Reminders);
  private readonly store = inject(WellbeingStore);
  private cfg: WellbeingReminderConfig = loadConfig();
  // Stored, so a removed rule's alarm is cancelled even after a reload.
  private armedIds = new Set<string>(loadArmedIds());

  get available(): boolean {
    return this.reminders.available;
  }

  getConfig(): WellbeingReminderConfig {
    return { rules: this.cfg.rules.map((r) => ({ ...r })) };
  }

  setConfig(config: WellbeingReminderConfig): void {
    this.cfg = { rules: config.rules.map((r) => ({ ...r })) };
    saveConfig(this.cfg);
    this.store.items$.pipe(take(1)).subscribe((items) => this.rearm(items));
  }

  /** Called once at startup. */
  init(): void {
    this.store.items$.subscribe((items) => this.rearm(items));
  }

  private rearm(items: readonly { recordedAt: string }[]): void {
    if (!this.reminders.available) return;
    const now = new Date();
    const last = lastCheckin(items);
    const wanted = new Set(this.cfg.rules.map((r) => r.id));
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
