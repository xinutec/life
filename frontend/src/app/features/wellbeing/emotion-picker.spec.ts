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
    const fixture = TestBed.createComponent(EmotionPicker);
    return { c: fixture.componentInstance, fixture, ref };
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

  it('selects a secondary group as an answer in its own right', () => {
    // "Frustrated" is often the whole truth; committing to Infuriated or Annoyed
    // would record the feeling as more precise than it was.
    const angry = EMOTION_WHEEL.find((core) => core.name === 'Angry')!;
    const { c } = setup([]);
    c.toggle('Angry/Frustrated');
    expect(c.isSelected('Angry/Frustrated')).toBe(true);
    expect(c.isSelected('Angry/Annoyed')).toBe(false); // no leaf implied
    expect(c.label('Angry/Frustrated')).toBe('Frustrated');
    expect(c.desc('Angry/Frustrated')).toBe('Blocked from what you want.');
    expect(c.coreCount(angry)).toBe(1); // a group counts toward its family badge
  });

  it('shows a group hit by family alone, a leaf hit by family and group', () => {
    const { c } = setup([]);
    c.query.set('frustrat');
    const [group, leaf] = c.results();
    expect(group.name).toBe('Frustrated');
    expect(c.path(group)).toBe('Angry');
    expect(c.path(leaf)).toBe('Angry › Frustrated');
  });

  it('the ⓘ opens a gloss without selecting, and the same ⓘ closes it', () => {
    const { c } = setup([]);

    c.toggleGloss('Happy/Aroused');
    expect(c.opened()).toBe('Happy/Aroused');
    expect(c.isSelected('Happy/Aroused')).toBe(false); // reading never selects

    c.toggleGloss('Happy/Aroused'); // same ⓘ again closes it
    expect(c.opened()).toBeNull();
  });

  it('opens only one gloss at a time', () => {
    // The gloss renders in place, so two open at once would push the mosaic
    // around; another word's ⓘ replaces the open one rather than adding to it.
    const { c } = setup([]);
    c.toggleGloss('Happy/Aroused');
    c.toggleGloss('Happy/Cheeky');
    expect(c.opened()).toBe('Happy/Cheeky');
  });

  it('counts every word in a family, both rings', () => {
    const happy = EMOTION_WHEEL.find((core) => core.name === 'Happy')!;
    const groups = happy.groups.length;
    const leaves = happy.groups.reduce((n, g) => n + g.leaves.length, 0);
    const { c } = setup([]);
    expect(c.wordCount(happy)).toBe(groups + leaves);
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
