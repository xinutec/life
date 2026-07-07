import { Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { MatBottomSheet, MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { map } from 'rxjs';

import { ListState } from '../../shared/list-state';
import { WellbeingCheckin, fatigueMeta, scoreMeta } from '../../shared/wellbeing-checkin';
import { WellbeingDoc, WellbeingStore } from '../../sync/wellbeing-store';
import { WellbeingEntry } from './wellbeing-entry';

interface Day {
  key: string;
  label: string;
  entries: WellbeingDoc[];
}

/** A dot in a 14-day trend chart (SVG user units). `level` is the 1..5 value
 *  (mood score or fatigue level) and drives the colour class. */
interface Dot {
  cx: number;
  cy: number;
  level: number;
}

const CHART = { w: 300, h: 96, padX: 6, padTop: 8, padBottom: 8, days: 14 };

/** Local calendar day key (YYYY-MM-DD) for grouping. */
function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Wellbeing history: a one-tap check-in strip, a 14-day trend, and a day-by-day
 *  timeline of entries. Tapping an entry opens the edit sheet. */
@Component({
  selector: 'app-wellbeing',
  templateUrl: './wellbeing.html',
  styleUrl: './wellbeing.scss',
  imports: [MatButtonModule, MatIconModule, MatBottomSheetModule, ListState, WellbeingCheckin],
})
export class Wellbeing {
  private store = inject(WellbeingStore);
  private sheet = inject(MatBottomSheet);

  readonly items = toSignal(this.store.items$, { initialValue: [] as WellbeingDoc[] });
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), { initialValue: false });

  /** Entries grouped by local day, newest day first (items$ is already desc). */
  readonly days = computed<Day[]>(() => {
    const groups = new Map<string, Day>();
    for (const e of this.items()) {
      const d = new Date(e.recordedAt);
      const key = dayKey(d);
      let g = groups.get(key);
      if (!g) {
        g = { key, label: this.dayLabel(d), entries: [] };
        groups.set(key, g);
      }
      g.entries.push(e);
    }
    return [...groups.values()];
  });

  /** The 14-day mood trend: a dot per entry, x = day column + time-of-day, y = score. */
  readonly chart = computed(() => this.buildChart((e) => e.score));
  readonly hasChart = computed(() => this.chart().dots.length > 0);

  /** The same 14-day trend for the optional fatigue reading, plotted on its
   *  stored `energy` value — so like mood, higher (energetic / no fatigue) sits at
   *  the top and a rising line reads as improving. Only entries that recorded one
   *  contribute, so it's absent until there's data. */
  readonly fatigueChart = computed(() => this.buildChart((e) => e.energy));
  readonly hasFatigueChart = computed(() => this.fatigueChart().dots.length > 0);

  /** Build a trend from a 1..5 accessor; entries returning null/undefined (e.g. an
   *  unrecorded fatigue) or falling outside the 14-day window are skipped. */
  private buildChart(value: (e: WellbeingDoc) => number | null | undefined) {
    const { w, h, padX, padTop, padBottom, days } = CHART;
    const plotH = h - padTop - padBottom;
    const colW = (w - 2 * padX) / days;
    // Oldest of the window = today - 13 (local midnight).
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    start.setDate(start.getDate() - (days - 1));
    const dots: Dot[] = [];
    for (const e of this.items()) {
      const level = value(e);
      if (level == null) continue; // no reading of this kind on this entry
      const d = new Date(e.recordedAt);
      const dayIdx = Math.floor((d.getTime() - start.getTime()) / 86_400_000);
      if (dayIdx < 0 || dayIdx >= days) continue; // outside the window
      const timeFrac = (d.getHours() * 60 + d.getMinutes()) / 1440;
      const cx = padX + colW * (dayIdx + 0.2 + 0.6 * timeFrac);
      const cy = padTop + ((5 - level) / 4) * plotH;
      dots.push({ cx: Math.round(cx * 10) / 10, cy: Math.round(cy * 10) / 10, level });
    }
    return { w, h, dots };
  }

  meta(score: number) {
    return scoreMeta(score);
  }

  fatigueMeta(score: number) {
    return fatigueMeta(score);
  }

  /** "14:05" — the entry's local clock time. */
  time(iso: string): string {
    return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  edit(entry: WellbeingDoc): void {
    this.sheet.open(WellbeingEntry, { data: { ulid: entry.ulid } });
  }

  /** "Today" / "Yesterday" / "Sat 5 Jul". */
  private dayLabel(d: Date): string {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const that = new Date(d);
    that.setHours(0, 0, 0, 0);
    const diff = Math.round((today.getTime() - that.getTime()) / 86_400_000);
    if (diff === 0) return 'Today';
    if (diff === 1) return 'Yesterday';
    return d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
  }
}
