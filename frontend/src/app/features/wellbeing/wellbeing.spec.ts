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
  scoreTenths: 30,
  energyTenths: null,
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
      entry({ ulid: 'a', recordedAt: at(0, 15), scoreTenths: 40 }),
      entry({ ulid: 'b', recordedAt: at(0, 9), scoreTenths: 20 }),
      entry({ ulid: 'c', recordedAt: at(1, 10), scoreTenths: 30 }),
    ];
    const days = setup(items).fixture.componentInstance.days();
    expect(days.length).toBe(2);
    expect(days[0].label).toBe('Today');
    expect(days[0].entries.map((e) => e.ulid)).toEqual(['a', 'b']);
    expect(days[1].label).toBe('Yesterday');
  });

  it('plots a chart dot per recent entry and none for old ones', () => {
    const items = [
      entry({ ulid: 'a', recordedAt: hoursAgo(2), scoreTenths: 50 }),
      entry({ ulid: 'z', recordedAt: at(40, 12), scoreTenths: 10 }), // outside the 14-day window
    ];
    const c = setup(items).fixture.componentInstance;
    expect(c.chart().dots.length).toBe(1);
    expect(c.hasChart()).toBe(true);
  });

  it('plots the energy chart only from entries that recorded one', () => {
    const c = setup([
      entry({ ulid: 'a', recordedAt: hoursAgo(2), scoreTenths: 40, energyTenths: 50 }),
      entry({ ulid: 'b', recordedAt: at(1, 12), scoreTenths: 30, energyTenths: null }),
      entry({ ulid: 'c', recordedAt: at(2, 12), scoreTenths: 20, energyTenths: 20 }),
    ]).fixture.componentInstance;
    expect(c.hasEnergyChart()).toBe(true);
    // Dots come out chronological (oldest→newest, left→right) so the line joins
    // them in time order: c (2 days ago, energy 2) then a (2h ago, energy 5);
    // b recorded no energy and is excluded. A 2 sits low, a 5 rides at the top.
    const [low, high] = c.energyChart().dots;
    expect(low.cy).toBeGreaterThan(high.cy);
    expect(high.cy).toBe(c.energyChart().levelY[0]); // a 5 is the top of the scale
  });

  it('plots a half-step between the two whole readings it sits between', () => {
    const c = setup([
      entry({ ulid: 'a', recordedAt: hoursAgo(3), scoreTenths: 30 }), // a 3
      entry({ ulid: 'b', recordedAt: hoursAgo(2), scoreTenths: 35 }), // a 3.5
      entry({ ulid: 'c', recordedAt: hoursAgo(1), scoreTenths: 40 }), // a 4
    ]).fixture.componentInstance;
    const [three, half, four] = c.chart().dots;
    // Halfway up, to within the 0.1-unit rounding the dots are stored at: it must
    // not land on either neighbour, or the reading he took the trouble to hedge is
    // lost. (A whole step is 18 units here, so 0.05 is nowhere near ambiguous.)
    expect(Math.abs(half.cy - (three.cy + four.cy) / 2)).toBeLessThanOrEqual(0.05);
    // ...and its colour is the blend of the two rungs, so height and hue agree.
    expect(half.fill).toContain('color-mix');
    expect(three.fill).toBe('var(--wb-score-3)');
  });

  it('rules off each local midnight in the window', () => {
    const c = setup([entry({ recordedAt: hoursAgo(2), scoreTenths: 40 })]).fixture.componentInstance;
    // A rolling 14-day window crosses 14 midnights whatever hour the suite runs at.
    c.window.set(14);
    const xs = c.chart().midnights;
    expect(xs.length).toBe(14);
    // Ascending and strictly inside the plot, so they read as day dividers.
    expect(xs).toEqual([...xs].sort((a, b) => a - b));
    expect(xs[0]).toBeGreaterThan(0);
    expect(xs[xs.length - 1]).toBeLessThan(c.chart().w);
    // The 24h window crosses exactly one: last night's.
    c.window.set(1);
    expect(c.chart().midnights.length).toBe(1);
  });

  it('names each day that is wide enough to hold the word', () => {
    const c = setup([entry({ recordedAt: hoursAgo(2), scoreTenths: 40 })]).fixture.componentInstance;
    // 14 days: each is ~20 units wide, too narrow for a name — rules only.
    c.window.set(14);
    expect(c.chart().midnights.length).toBe(14);
    expect(c.chart().dayLabels).toEqual([]);

    // 7 days: the 6 whole days always fit; the part-days at either edge fit only
    // if the window opened early enough in the day, so the count is 6, 7 or 8.
    c.window.set(7);
    const week = c.chart().dayLabels;
    expect(week.length).toBeGreaterThanOrEqual(6);
    expect(week.length).toBeLessThanOrEqual(8);
    // Each name is centred in its own day: strictly between the rules either side.
    const rules = [0, ...c.chart().midnights, c.chart().w];
    for (const d of week) {
      const left = Math.max(...rules.filter((r) => r < d.x));
      const right = Math.min(...rules.filter((r) => r > d.x));
      expect(right - left).toBeGreaterThan(0);
    }
    // A real weekday, in the user's locale.
    const weekdays = [0, 1, 2, 3, 4, 5, 6].map((i) =>
      new Date(2024, 0, 7 + i).toLocaleDateString(undefined, { weekday: 'short' }),
    );
    for (const d of week) expect(weekdays).toContain(d.text);
  });

  it('has no energy chart when nothing recorded one', () => {
    const c = setup([entry({ energyTenths: null })]).fixture.componentInstance;
    expect(c.hasChart()).toBe(true); // mood still charts
    expect(c.hasEnergyChart()).toBe(false);
  });

  // Panning. The charts keep a fixed viewBox and redraw the window in place, so
  // these assert the *window* moves — never that the drawing grows.
  describe('panning back through history', () => {
    /** The scroll overlay, at `frac` of the way along its rail. Only the three
     *  measurements onPan reads; no DOM needed to exercise the mapping. */
    const scroller = (frac: number, factor: number): HTMLElement =>
      ({
        clientWidth: 300,
        scrollWidth: 300 * factor,
        scrollLeft: frac * 300 * (factor - 1),
      }) as HTMLElement;

    /** A month of daily check-ins, newest first, as the store supplies them. */
    const month = (): WellbeingDoc[] =>
      Array.from({ length: 30 }, (_, i) =>
        entry({
          ulid: `d${i}`,
          recordedAt: hoursAgo(i * 24 + 2),
          scoreTenths: 10 + ((i * 10) % 41),
        }),
      );

    it('starts pinned to now, showing one window of a pannable month', () => {
      const c = setup(month()).fixture.componentInstance;
      expect(c.atNow()).toBe(true);
      expect(c.chart().dots.length).toBe(7); // one week of daily entries
      expect(c.panFactor()).toBeGreaterThan(4); // ~30 days of history, 7 to a screen
    });

    it('moves the window back without drawing more of the history', () => {
      const c = setup(month()).fixture.componentInstance;
      const pinned = c.chart().dots.length;
      c.onPan(scroller(0, c.panFactor())); // hard left
      expect(c.atNow()).toBe(false);
      expect(c.endMs()).toBeLessThan(Date.now() - 20 * 86_400_000);
      // Still a window's worth of dots — the whole point of the exercise.
      expect(c.chart().dots.length).toBeLessThanOrEqual(pinned + 1);
    });

    it('stops at the oldest entry and re-pins at the right edge', () => {
      const items = month();
      const c = setup(items).fixture.componentInstance;
      c.onPan(scroller(0, c.panFactor()));
      // Panned fully left, the window opens on the first check-in, not before it.
      const oldest = new Date(items[items.length - 1].recordedAt).getTime();
      expect(c.endMs() - 7 * 86_400_000).toBeGreaterThanOrEqual(oldest - 1000);
      c.onPan(scroller(1, c.panFactor()));
      expect(c.atNow()).toBe(true);
    });

    it('does not scroll at all when the history already fits one window', () => {
      const c = setup([entry({ recordedAt: hoursAgo(2) })]).fixture.componentInstance;
      expect(c.pannableMs()).toBe(0);
      expect(c.panFactor()).toBe(1);
    });

    it('carries the trend line past both edges of the window', () => {
      const c = setup(month()).fixture.componentInstance;
      c.onPan(scroller(0.5, c.panFactor()));
      const { dots, line } = c.chart();
      expect(line.length).toBe(dots.length + 4); // two readings either side
      // The halo is genuinely outside the window, which is what carries the curve
      // off-screen instead of ending it at the last visible dot.
      expect(line[0].cx).toBeLessThan(dots[0].cx);
      expect(line[line.length - 1].cx).toBeGreaterThan(dots[dots.length - 1].cx);
    });

    it('keeps the chart on screen over an empty stretch, so panning is reversible', () => {
      // Two check-ins two months apart: pan into the gap between them.
      const c = setup([
        entry({ ulid: 'new', recordedAt: hoursAgo(2) }),
        entry({ ulid: 'old', recordedAt: hoursAgo(60 * 24) }),
      ]).fixture.componentInstance;
      c.onPan(scroller(0.5, c.panFactor()));
      expect(c.chart().dots.length).toBe(0);
      // If the chart went away with its dots it would take the scroller with it,
      // and there would be no way back to now.
      expect(c.hasChart()).toBe(true);
      expect(c.emptyWindow()).toBe(true);
      // The line still crosses the gap: "nothing here", not "no data exists".
      expect(c.chart().line.length).toBe(2);
    });

    it('names the range once panned, since "last 7 days" is then untrue', () => {
      const c = setup(month()).fixture.componentInstance;
      expect(c.windowLabel()).toBe('last 7 days');
      c.onPan(scroller(0.3, c.panFactor()));
      expect(c.windowLabel()).not.toContain('last');
      expect(c.windowLabel()).toContain('–');
    });

    /** The rail holds a whole number of pixels, and at a 14-day window on a phone
     *  ONE PIXEL IS ABOUT 50 MINUTES. So the re-seat writes a fractional position,
     *  the rail keeps a rounded one, and reading that back names a different
     *  instant — up to ~25 minutes away. Landed near local midnight, the caption
     *  (day names, both ends) jumps a whole day. That is #1293, and it is not a
     *  race: the scroll below moves the rail zero pixels.
     *
     *  jsdom has no layout, so the geometry a phone gives the rail is supplied
     *  here, ROUNDING on write exactly as a browser does. The rounding IS the
     *  fault under test — without it this test cannot fail. */
    it('ignores a scroll reporting the position the re-seat itself left', async () => {
      const { fixture } = setup(month());
      const c = fixture.componentInstance;
      fixture.detectChanges();
      const host = fixture.nativeElement as HTMLElement;
      const el = host.querySelector<HTMLElement>('.pan');
      if (!el) throw new Error('the pan rail did not render');
      let left = 0;
      Object.defineProperty(el, 'clientWidth', { get: () => 380 });
      Object.defineProperty(el, 'scrollWidth', { get: () => Math.round(380 * c.panFactor()) });
      Object.defineProperty(el, 'scrollLeft', {
        get: () => left,
        set: (v: number) => {
          left = Math.round(v);
        },
      });

      // Pan back through the history, then zoom: the case that re-seats the rail.
      el.scrollLeft = (el.scrollWidth - el.clientWidth) * 0.5;
      c.onPan(el);
      c.window.set(14);
      fixture.detectChanges();
      await fixture.whenStable();

      const settled = c.endMs();
      const seated = el.scrollLeft;
      c.onPan(el); // the scroll event a re-seat provokes: nothing has moved
      expect(el.scrollLeft).toBe(seated);
      expect(c.endMs()).toBe(settled);
    });

    it('goes back to now on demand', () => {
      const c = setup(month()).fixture.componentInstance;
      c.onPan(scroller(0.2, c.panFactor()));
      expect(c.atNow()).toBe(false);
      c.toNow();
      expect(c.atNow()).toBe(true);
      expect(c.windowLabel()).toBe('last 7 days');
    });
  });

  it('opens the edit sheet for an entry', () => {
    const { fixture, sheet } = setup([entry({ ulid: 'a' })]);
    fixture.componentInstance.edit(entry({ ulid: 'a' }));
    expect(sheet.open).toHaveBeenCalled();
  });

  it('shows an energy icon only on entries that recorded one', () => {
    const { fixture } = setup([
      entry({ ulid: 'a', recordedAt: at(0, 15), energyTenths: 20 }), // energy 2 → "low"
      entry({ ulid: 'b', recordedAt: at(0, 9), energyTenths: null }),
    ]);
    fixture.detectChanges();
    const host = fixture.nativeElement as HTMLElement;
    const icons = host.querySelectorAll('.has-energy');
    expect(icons.length).toBe(1);
    expect(icons[0].textContent?.trim()).toBe('battery_2_bar'); // energy 2 → low
  });
});
