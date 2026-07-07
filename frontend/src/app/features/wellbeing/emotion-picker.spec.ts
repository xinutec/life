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

  it('seeds from the passed selection', () => {
    const { c } = setup(['Withdrawn']);
    expect(c.isSelected('Withdrawn')).toBe(true);
    expect(c.isSelected('Numb')).toBe(false);
    expect(c.count()).toBe(1);
  });

  it('toggles a leaf on and off', () => {
    const { c } = setup([]);
    c.toggle('Anxious');
    expect(c.isSelected('Anxious')).toBe(true);
    expect(c.selectedList()).toEqual(['Anxious']);
    c.toggle('Anxious');
    expect(c.isSelected('Anxious')).toBe(false);
    expect(c.count()).toBe(0);
  });

  it('counts selected leaves per core for the panel badge', () => {
    const angry = EMOTION_WHEEL.find((core) => core.name === 'Angry')!;
    const { c } = setup(['Withdrawn', 'Numb', 'Anxious']); // 2 Angry, 1 Fearful
    expect(c.coreCount(angry)).toBe(2);
  });

  it('Done closes with the new set; Cancel closes with undefined', () => {
    const { c, ref } = setup(['Withdrawn']);
    c.toggle('Numb');
    c.done();
    expect(ref.close).toHaveBeenCalledWith(['Withdrawn', 'Numb']);

    c.cancel();
    expect(ref.close).toHaveBeenLastCalledWith(undefined);
  });
});
