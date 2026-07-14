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
      vi.advanceTimersByTime(10_000); // past the window — the snackbar is long gone
      await c.log(3);
      expect(store.add).toHaveBeenCalledTimes(2);
      expect(store.patch).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
