import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { WellbeingStore, WellbeingDoc } from '../../sync/wellbeing-store';
import { Wellbeing } from './wellbeing';

/** ISO instant `daysAgo` days back at a given local time (for day-grouping). */
const at = (daysAgo: number, h: number, m = 0): string => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  return d.toISOString();
};

/** ISO instant `n` hours back — unambiguously in the past, so the rolling chart
 *  window (which ends at "now") always includes it whatever time the suite runs. */
const hoursAgo = (n: number): string => new Date(Date.now() - n * 3_600_000).toISOString();

const entry = (over: Partial<WellbeingDoc>): WellbeingDoc => ({
  ulid: 'u',
  id: 1,
  recordedAt: hoursAgo(2),
  score: 3,
  energy: null,
  emotions: [],
  note: null,
  rev: 1,
  ...over,
});

describe('Wellbeing history', () => {
  function setup(items: WellbeingDoc[]) {
    const store = { items$: of(items), syncError: signal<string | null>(null) };
    const sheet = { open: vi.fn() };
    TestBed.configureTestingModule({
      imports: [Wellbeing],
      providers: [{ provide: WellbeingStore, useValue: store }],
    });
    TestBed.overrideProvider(MatBottomSheet, { useValue: sheet });
    return { fixture: TestBed.createComponent(Wellbeing), sheet };
  }

  it('groups entries by local day, newest day first', () => {
    // Provided newest-first, as the store sorts them.
    const items = [
      entry({ ulid: 'a', recordedAt: at(0, 15), score: 4 }),
      entry({ ulid: 'b', recordedAt: at(0, 9), score: 2 }),
      entry({ ulid: 'c', recordedAt: at(1, 10), score: 3 }),
    ];
    const days = setup(items).fixture.componentInstance.days();
    expect(days.length).toBe(2);
    expect(days[0].label).toBe('Today');
    expect(days[0].entries.map((e) => e.ulid)).toEqual(['a', 'b']);
    expect(days[1].label).toBe('Yesterday');
  });

  it('plots a chart dot per recent entry and none for old ones', () => {
    const items = [
      entry({ ulid: 'a', recordedAt: hoursAgo(2), score: 5 }),
      entry({ ulid: 'z', recordedAt: at(40, 12), score: 1 }), // outside the 14-day window
    ];
    const c = setup(items).fixture.componentInstance;
    expect(c.chart().dots.length).toBe(1);
    expect(c.hasChart()).toBe(true);
  });

  it('plots the energy chart only from entries that recorded one', () => {
    const c = setup([
      entry({ ulid: 'a', recordedAt: hoursAgo(2), score: 4, energy: 5 }),
      entry({ ulid: 'b', recordedAt: at(1, 12), score: 3, energy: null }),
      entry({ ulid: 'c', recordedAt: at(2, 12), score: 2, energy: 2 }),
    ]).fixture.componentInstance;
    expect(c.hasEnergyChart()).toBe(true);
    // Dots come out chronological (oldest→newest, left→right) so the line joins
    // them in time order: c (2 days ago, energy 2) then a (2h ago, energy 5);
    // b recorded no energy and is excluded.
    expect(c.energyChart().dots.map((d) => d.level)).toEqual([2, 5]);
  });

  it('has no energy chart when nothing recorded one', () => {
    const c = setup([entry({ energy: null })]).fixture.componentInstance;
    expect(c.hasChart()).toBe(true); // mood still charts
    expect(c.hasEnergyChart()).toBe(false);
  });

  it('opens the edit sheet for an entry', () => {
    const { fixture, sheet } = setup([entry({ ulid: 'a' })]);
    fixture.componentInstance.edit(entry({ ulid: 'a' }));
    expect(sheet.open).toHaveBeenCalled();
  });

  it('shows an energy icon only on entries that recorded one', () => {
    const { fixture } = setup([
      entry({ ulid: 'a', recordedAt: at(0, 15), energy: 2 }), // energy 2 → "low"
      entry({ ulid: 'b', recordedAt: at(0, 9), energy: null }),
    ]);
    fixture.detectChanges();
    const host = fixture.nativeElement as HTMLElement;
    const icons = host.querySelectorAll('.has-energy');
    expect(icons.length).toBe(1);
    expect(icons[0].textContent?.trim()).toBe('battery_2_bar'); // energy 2 → low
  });
});
