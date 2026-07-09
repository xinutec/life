import { TestBed } from '@angular/core/testing';
import {
  MAT_BOTTOM_SHEET_DATA,
  MatBottomSheetRef,
} from '@angular/material/bottom-sheet';
import { MatDialog } from '@angular/material/dialog';
import { BehaviorSubject } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { WellbeingStore, WellbeingDoc } from '../../sync/wellbeing-store';
import { WellbeingEntry } from './wellbeing-entry';

const doc = (over: Partial<WellbeingDoc>): WellbeingDoc => ({
  ulid: 'u1',
  id: 1,
  recordedAt: '2026-07-01T12:00:00.000Z',
  score: 3,
  energy: null,
  emotions: [],
  note: null,
  rev: 1,
  ...over,
});

describe('WellbeingEntry (edit sheet)', () => {
  function setup(initial: WellbeingDoc) {
    const items$ = new BehaviorSubject<WellbeingDoc[]>([initial]);
    const store = {
      items$,
      patch: vi.fn().mockResolvedValue(undefined),
      remove: vi.fn().mockResolvedValue(undefined),
      undoDelete: vi.fn().mockResolvedValue(undefined),
    };
    const ref = { dismiss: vi.fn() };
    const feedback = {
      undo: vi.fn<(message: string, onUndo: () => void, onCommit?: () => void) => void>(),
    };
    const dialog = { open: vi.fn() };
    TestBed.configureTestingModule({
      imports: [WellbeingEntry],
      providers: [
        { provide: WellbeingStore, useValue: store },
        { provide: MatBottomSheetRef, useValue: ref },
        { provide: MAT_BOTTOM_SHEET_DATA, useValue: { ulid: initial.ulid } },
        { provide: Feedback, useValue: feedback },
        { provide: MatDialog, useValue: dialog },
      ],
    });
    const fixture = TestBed.createComponent(WellbeingEntry);
    return { c: fixture.componentInstance, fixture, store, ref, feedback, items$ };
  }

  it('patches the score on tap', () => {
    const { c, store } = setup(doc({ score: 3 }));
    c.setScore(5);
    expect(store.patch).toHaveBeenCalledWith('u1', { score: 5 });
  });

  it('sets energy on tap', () => {
    const { c, store } = setup(doc({ energy: null }));
    c.setEnergy(4); // the level button passes its energy value
    expect(store.patch).toHaveBeenCalledWith('u1', { energy: 4 });
  });

  it('toggles energy back to null when the active level is tapped again', () => {
    const { c, store } = setup(doc({ energy: 4 }));
    c.setEnergy(4);
    expect(store.patch).toHaveBeenCalledWith('u1', { energy: null });
  });

  it('removes one emotion, leaving the rest', () => {
    const { c, store } = setup(doc({ emotions: ['Withdrawn', 'Anxious', 'Numb'] }));
    c.removeEmotion('Anxious');
    expect(store.patch).toHaveBeenCalledWith('u1', { emotions: ['Withdrawn', 'Numb'] });
  });

  it('round-trips the datetime-local value back to a UTC instant', () => {
    const { c, store } = setup(doc({}));
    // localTime() renders the stored instant as a local wall-clock value…
    const local = c.localTime();
    // …and feeding that same value back stores the identical instant.
    c.setTime(local);
    expect(store.patch).toHaveBeenCalledWith('u1', {
      recordedAt: '2026-07-01T12:00:00.000Z',
    });
  });

  it('ignores a cleared time input', () => {
    const { c, store } = setup(doc({}));
    c.setTime('');
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('saves a trimmed note, empty → null', () => {
    const { c, store } = setup(doc({ note: 'old' }));
    c.onNoteInput('  new note ');
    c.saveNote();
    expect(store.patch).toHaveBeenCalledWith('u1', { note: 'new note' });
    c.onNoteInput('   ');
    c.saveNote();
    expect(store.patch).toHaveBeenCalledWith('u1', { note: null });
  });

  it('flushes an unsaved note edit when the sheet is dismissed', () => {
    const { c, fixture, store } = setup(doc({ note: null }));
    c.onNoteInput('typed but never blurred');
    fixture.destroy();
    expect(store.patch).toHaveBeenCalledWith('u1', { note: 'typed but never blurred' });
  });

  it('does not flush an unchanged note on dismiss', () => {
    const { c, fixture, store } = setup(doc({ note: 'same' }));
    c.onNoteInput('same');
    fixture.destroy();
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('does not flush the seeded note when a remote edit arrives (no typing)', () => {
    // The sheet opened on a note; another device changed it while open and the
    // user never touched the field. Dismissing must NOT write the stale original
    // back over the remote edit.
    const { fixture, store, items$ } = setup(doc({ note: 'original' }));
    items$.next([doc({ note: 'edited elsewhere' })]);
    fixture.destroy();
    expect(store.patch).not.toHaveBeenCalled();
  });

  it('delete removes, dismisses, offers Undo — and skips the note flush', () => {
    const { c, fixture, store, ref, feedback } = setup(doc({ note: null }));
    c.onNoteInput('half-typed'); // dirty — would otherwise be flushed by ngOnDestroy
    c.remove();
    fixture.destroy();
    expect(store.remove).toHaveBeenCalledWith('u1');
    expect(ref.dismiss).toHaveBeenCalled();
    expect(feedback.undo).toHaveBeenCalledWith('Check-in deleted', expect.any(Function));
    expect(store.patch).not.toHaveBeenCalled(); // the deleting guard held
  });

  it('Undo restores the deleted doc (local revive + server trash restore)', () => {
    const d = doc({ score: 2, note: 'rough' });
    const { c, store, feedback } = setup(d);
    c.remove();
    const [, undoFn] = feedback.undo.mock.calls[0];
    undoFn();
    expect(store.undoDelete).toHaveBeenCalledWith(d);
  });
});
