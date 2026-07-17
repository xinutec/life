import { DOCUMENT } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { NavigationEnd, Router } from '@angular/router';
import { Subject } from 'rxjs';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LifeApi } from './life-api';
import { Telemetry, labelFor } from './telemetry';

describe('labelFor', () => {
  function el(html: string): Element {
    const host = document.createElement('div');
    host.innerHTML = html;
    return host.firstElementChild!;
  }

  it('reads a button’s visible text, whitespace collapsed', () => {
    const b = el('<button>  Find at\n  Asda </button>');
    expect(labelFor(b)).toBe('Find at Asda');
  });

  it('prefers an explicit aria-label over the text', () => {
    const b = el('<button aria-label="Add Asda listing">Add</button>');
    expect(labelFor(b)).toBe('Add Asda listing');
  });

  it('climbs to the enclosing control when the tap lands on an inner icon', () => {
    // Material buttons wrap an icon + label; a tap often hits the icon span.
    const b = el('<button><mat-icon>store</mat-icon><span>Find at Asda</span></button>');
    const icon = b.querySelector('mat-icon')!;
    expect(labelFor(icon)).toBe('Find at Asda');
  });

  it('returns null for a tap on nothing interactive', () => {
    const p = el('<p>just some copy</p>');
    expect(labelFor(p)).toBeNull();
    expect(labelFor(null)).toBeNull();
  });

  it('recognises role-based controls, not just <button>', () => {
    expect(labelFor(el('<div role="button">Save</div>'))).toBe('Save');
    expect(labelFor(el('<a role="tab">Today</a>'))).toBe('Today');
  });
});

describe('Telemetry', () => {
  let events: Subject<unknown>;
  let sent: unknown[][];
  let router: { events: Subject<unknown>; url: string };

  beforeEach(() => {
    vi.useFakeTimers();
    events = new Subject();
    sent = [];
    router = { events, url: '/today' };
    const api = {
      sendTelemetry: vi.fn((batch: unknown[]) => {
        sent.push(batch);
        return of(undefined);
      }),
    };
    TestBed.configureTestingModule({
      providers: [
        Telemetry,
        { provide: Router, useValue: router },
        { provide: LifeApi, useValue: api },
        { provide: DOCUMENT, useValue: document },
      ],
    });
    TestBed.inject(Telemetry).init();
  });

  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  it('records a navigation, then flushes it on the timer', () => {
    events.next(new NavigationEnd(1, '/product/56', '/product/56'));
    expect(sent).toHaveLength(0); // nothing sent until a flush
    vi.advanceTimersByTime(5000);

    expect(sent).toHaveLength(1);
    const batch = sent[0] as { kind: string; path: string; label: string | null }[];
    expect(batch).toHaveLength(1);
    expect(batch[0]).toMatchObject({ kind: 'nav', path: '/product/56', label: null });
  });

  it('records a tap with the control’s label at the current route', () => {
    router.url = '/product/56';
    const btn = document.createElement('button');
    btn.textContent = 'Find at Asda';
    document.body.appendChild(btn);
    btn.click();
    vi.advanceTimersByTime(5000);
    document.body.removeChild(btn);

    const batch = sent[0] as { kind: string; path: string; label: string | null }[];
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

    expect(sent[0]).toHaveLength(1); // one nav, not two
  });
});
