import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from './feedback';
import { WellbeingCheckin } from './wellbeing-checkin';
import { WellbeingStore } from '../sync/wellbeing-store';

describe('WellbeingCheckin', () => {
  function setup() {
    const store = {
      add: vi.fn<
        (input: { recordedAt: string; scoreTenths: number; note: string | null }) => Promise<string>
      >(() => Promise.resolve('u1')),
      patch: vi.fn().mockResolvedValue(undefined),
      remove: vi.fn(),
    };
    const feedback = { undo: vi.fn<(msg: string, onUndo: () => void) => void>() };
    TestBed.configureTestingModule({
      imports: [WellbeingCheckin],
      providers: [
        { provide: WellbeingStore, useValue: store },
        { provide: Feedback, useValue: feedback },
      ],
    });
    return { fixture: TestBed.createComponent(WellbeingCheckin), store, feedback };
  }

  it('logs a check-in at "now" with the tapped score and offers Undo', async () => {
    const { fixture, store, feedback } = setup();
    await fixture.componentInstance.log(4);
    expect(store.add).toHaveBeenCalledTimes(1);
    const [input] = store.add.mock.calls[0];
    expect(input.scoreTenths).toBe(40); // tenths: a 4 is a 40
    expect(input.note).toBeNull();
    expect(typeof input.recordedAt).toBe('string');
    expect(feedback.undo).toHaveBeenCalled();
  });

  it('Undo removes the just-created entry', async () => {
    const { fixture, store, feedback } = setup();
    await fixture.componentInstance.log(1);
    // Invoke the onUndo callback the component handed to Feedback.undo.
    const onUndo = feedback.undo.mock.calls[0][1];
    onUndo();
    expect(store.remove).toHaveBeenCalledWith('u1');
  });

  it('tapping a neighbouring face amends the entry to the half-step between', async () => {
    const { fixture, store } = setup();
    const c = fixture.componentInstance;
    await c.log(4);
    await c.log(3); // "4 — no, a bit lower than that"
    // ONE entry, amended — not a 4 and then a 3, which would say the mood dropped.
    expect(store.add).toHaveBeenCalledTimes(1);
    expect(store.patch).toHaveBeenCalledWith('u1', { scoreTenths: 35 });
  });

  it('swallows a stray double tap on the same face', async () => {
    const { fixture, store } = setup();
    const c = fixture.componentInstance;
    await c.log(4);
    await c.log(4); // the same face again — never means "log a second identical 4"
    expect(store.add).toHaveBeenCalledTimes(1);
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('tapping a face two or more away is a second check-in, not a half-step', async () => {
    const { fixture, store } = setup();
    const c = fixture.componentInstance;
    await c.log(2);
    await c.log(5); // not adjacent: this is a new feeling (or a fat-fingered fix)
    expect(store.add).toHaveBeenCalledTimes(2);
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('does not amend an entry the user already undid', async () => {
    const { fixture, store, feedback } = setup();
    const c = fixture.componentInstance;
    await c.log(4);
    feedback.undo.mock.calls[0][1](); // Undo: the entry is gone
    await c.log(3);
    // The removed entry must not be resurrected by a patch — the second tap is
    // simply a fresh check-in.
    expect(store.patch).not.toHaveBeenCalled();
    expect(store.add).toHaveBeenCalledTimes(2);
  });

  it('lets the amend window lapse: a later neighbour is its own check-in', async () => {
    vi.useFakeTimers();
    try {
      const { fixture, store } = setup();
      const c = fixture.componentInstance;
      await c.log(4);
      vi.advanceTimersByTime(90_000); // past the minute — this is a new feeling now
      await c.log(3);
      expect(store.add).toHaveBeenCalledTimes(2);
      expect(store.patch).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('WellbeingCheckin — saying more than a score', () => {
  function setup() {
    const store = {
      add: vi.fn<
        (input: { recordedAt: string; scoreTenths: number; note: string | null }) => Promise<string>
      >(() => Promise.resolve('u1')),
      patch: vi.fn().mockResolvedValue(undefined),
      remove: vi.fn(),
    };
    const feedback = { undo: vi.fn<(msg: string, onUndo: () => void) => void>() };
    TestBed.configureTestingModule({
      imports: [WellbeingCheckin],
      providers: [
        { provide: WellbeingStore, useValue: store },
        { provide: Feedback, useValue: feedback },
      ],
    });
    return { fixture: TestBed.createComponent(WellbeingCheckin), store, feedback };
  }

  it('offers a way into the entry it just logged', async () => {
    // Logging a bare score is the RARE case: 196 of 207 entries were edited
    // after creation, and a trace caught the edit tap landing in the same batch
    // as the log. Without this the only route back in is to find the entry in
    // the timeline and press Edit.
    const { fixture } = setup();
    const cmp = fixture.componentInstance;
    expect(cmp.justLogged()).toBeNull();
    await cmp.log(4);
    expect(cmp.justLogged()).toBe('u1');
  });

  it('takes the offer away when the check-in is undone', async () => {
    // It points at an entry that no longer exists; leaving it would open a sheet
    // onto a removed row.
    const { fixture, feedback } = setup();
    const cmp = fixture.componentInstance;
    await cmp.log(4);
    feedback.undo.mock.calls[0][1](); // the Undo tap
    expect(cmp.justLogged()).toBeNull();
  });

  it('still records a half-step, which is what an auto-opened sheet would have cost', async () => {
    // The reason this is a button under the strip and not a sheet that opens
    // itself: tapping an adjacent face inside the amend window is how a 3.5 is
    // recorded, and a sheet over the strip would take that gesture away.
    const { fixture, store } = setup();
    const cmp = fixture.componentInstance;
    await cmp.log(4);
    await cmp.log(3);
    expect(store.patch).toHaveBeenCalledWith('u1', { scoreTenths: 35 });
    expect(cmp.justLogged()).toBe('u1');
  });

  it('hides the offer unless the screen can act on it', () => {
    // Today shows the same strip and has no edit sheet; a button that leads
    // nowhere is worse than no button.
    const { fixture } = setup();
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.add-detail')).toBeNull();
  });
});

describe('WellbeingCheckin — the offer does not outlive its moment', () => {
  it('withdraws the offer once the amend window lapses', async () => {
    // Found by looking at the render: nothing cleared it, so an hour later the
    // strip would still be inviting you to say more about a finished check-in.
    vi.useFakeTimers();
    try {
      const store = {
        add: vi.fn(() => Promise.resolve('u1')),
        patch: vi.fn().mockResolvedValue(undefined),
        remove: vi.fn(),
      };
      TestBed.configureTestingModule({
        imports: [WellbeingCheckin],
        providers: [
          { provide: WellbeingStore, useValue: store },
          { provide: Feedback, useValue: { undo: vi.fn() } },
        ],
      });
      const c = TestBed.createComponent(WellbeingCheckin).componentInstance;
      await c.log(4);
      expect(c.justLogged()).toBe('u1');
      vi.advanceTimersByTime(90_000);
      expect(c.justLogged()).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
