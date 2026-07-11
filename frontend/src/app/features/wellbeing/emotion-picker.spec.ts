import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { describe, expect, it, vi } from 'vitest';

import { EMOTION_WHEEL } from '../../shared/emotion-wheel';
import { EmotionPicker } from './emotion-picker';

describe('EmotionPicker', () => {
  function setup(selected: string[]) {
    const ref = { close: vi.fn() };
    TestBed.configureTestingModule({
      imports: [EmotionPicker],
      providers: [
        { provide: MatDialogRef, useValue: ref },
        { provide: MAT_DIALOG_DATA, useValue: { selected } },
      ],
    });
    return { c: TestBed.createComponent(EmotionPicker).componentInstance, ref };
  }

  it('seeds from a token selection', () => {
    const { c } = setup(['Angry/Withdrawn']);
    expect(c.isSelected('Angry/Withdrawn')).toBe(true);
    expect(c.isSelected('Angry/Numb')).toBe(false);
    expect(c.count()).toBe(1);
  });

  it('upgrades a legacy bare word to its token when seeding', () => {
    const { c } = setup(['Withdrawn']); // pre-qualification stored value
    expect(c.isSelected('Angry/Withdrawn')).toBe(true);
    expect(c.selectedList()).toEqual(['Angry/Withdrawn']);
  });

  it('toggles a token on and off', () => {
    const { c } = setup([]);
    c.toggle('Fearful/Anxious');
    expect(c.isSelected('Fearful/Anxious')).toBe(true);
    expect(c.selectedList()).toEqual(['Fearful/Anxious']);
    c.toggle('Fearful/Anxious');
    expect(c.isSelected('Fearful/Anxious')).toBe(false);
    expect(c.count()).toBe(0);
  });

  it('keeps a same-named leaf under two cores independently selectable', () => {
    // "Overwhelmed" sits under both Fearful and Bad — selecting one must not
    // light up the other (the whole point of qualified tokens).
    const { c } = setup([]);
    c.toggle('Fearful/Overwhelmed');
    expect(c.isSelected('Fearful/Overwhelmed')).toBe(true);
    expect(c.isSelected('Bad/Overwhelmed')).toBe(false);
    expect(c.count()).toBe(1);
  });

  it('counts selected tokens per core for the panel badge', () => {
    const angry = EMOTION_WHEEL.find((core) => core.name === 'Angry')!;
    // 2 Angry, 1 Fearful — coreCount(Angry) must ignore the Fearful one.
    const { c } = setup(['Angry/Withdrawn', 'Angry/Numb', 'Fearful/Anxious']);
    expect(c.coreCount(angry)).toBe(2);
  });

  it('Done closes with the new token set; Cancel closes with undefined', () => {
    const { c, ref } = setup(['Angry/Withdrawn']);
    c.toggle('Angry/Numb');
    c.done();
    expect(ref.close).toHaveBeenCalledWith(['Angry/Withdrawn', 'Angry/Numb']);

    c.cancel();
    expect(ref.close).toHaveBeenLastCalledWith(undefined);
  });
});
