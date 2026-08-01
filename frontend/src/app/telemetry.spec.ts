import { DOCUMENT } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { NavigationEnd, Router } from '@angular/router';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Telemetry } from './telemetry';

/**
 * The activity trace's wiring, at a real consumer.
 *
 * The label rules and the flattener live in `@xinutec/ui-harness/telemetry`
 * and are tested there, once for the fleet — they are pure functions and need
 * no framework. What can only be checked from inside an app is the rest: that
 * the service actually reaches the router and the document it was given, that
 * the queue flushes on its timer, and that a send failure stays silent.
 *
 * life is the reference consumer, so this is the one place it is checked.
 */
describe('Telemetry (wiring)', () => {
  let events: Subject<unknown>;
  let router: { events: Subject<unknown>; url: string };
  let sent: { url: string; body: unknown }[];

  beforeEach(() => {
    vi.useFakeTimers();
    events = new Subject();
    router = { events, url: '/today' };
    sent = [];

    vi.stubGlobal(
      'fetch',
      vi.fn((url: string, init: { body: string }) => {
        sent.push({ url, body: JSON.parse(init.body) });
        return Promise.resolve(new Response(null, { status: 204 }));
      }),
    );

    TestBed.configureTestingModule({
      providers: [
        Telemetry,
        { provide: Router, useValue: router },
        { provide: DOCUMENT, useValue: document },
      ],
    });
    TestBed.inject(Telemetry).init();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  it('records a navigation, then flushes it on the timer', () => {
    events.next(new NavigationEnd(1, '/product/56', '/product/56'));
    expect(sent).toHaveLength(0); // nothing sent until a flush
    vi.advanceTimersByTime(5000);

    expect(sent).toHaveLength(1);
    expect(sent[0].url).toBe('/api/telemetry');
    const batch = sent[0].body as { kind: string; path: string; label: string | null }[];
    expect(batch).toHaveLength(1);
    expect(batch[0]).toMatchObject({ kind: 'nav', path: '/product/56', label: null });
  });

  it("records a tap with the control's label at the current route", () => {
    router.url = '/product/56';
    const btn = document.createElement('button');
    btn.textContent = 'Find at Asda';
    document.body.appendChild(btn);
    btn.click();
    vi.advanceTimersByTime(5000);
    document.body.removeChild(btn);

    const batch = sent[0].body as { kind: string; path: string; label: string | null }[];
    expect(batch[0]).toMatchObject({ kind: 'tap', path: '/product/56', label: 'Find at Asda' });
  });

  it('ignores taps that miss every control', () => {
    const p = document.createElement('p');
    p.textContent = 'not a button';
    document.body.appendChild(p);
    p.click();
    vi.advanceTimersByTime(5000);
    document.body.removeChild(p);

    expect(sent).toHaveLength(0);
  });

  it('is idempotent: a second init() does not double-wire the capture', () => {
    TestBed.inject(Telemetry).init(); // second call
    events.next(new NavigationEnd(1, '/todo', '/todo'));
    vi.advanceTimersByTime(5000);

    expect(sent[0].body).toHaveLength(1); // one nav, not two
  });

  it('swallows a failed send — a trace must never surface its own errors', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.reject(new Error('offline'))),
    );
    events.next(new NavigationEnd(1, '/todo', '/todo'));
    // The rejection is handled inside flush(); nothing escapes to the caller,
    // and an unhandled rejection here would fail the run.
    expect(() => vi.advanceTimersByTime(5000)).not.toThrow();
    await Promise.resolve();
  });
});
