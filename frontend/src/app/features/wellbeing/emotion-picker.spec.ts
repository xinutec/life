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

  describe('blends', () => {
    const disheartened = 'Sad/Disheartened';
    const deflated = 'Sad/Deflated';
    const blend = `${disheartened}+${deflated}`;

    it('offers to fuse two words of one group', () => {
      const { c } = setup([]);
      c.toggle(disheartened);
      expect(c.fusable()).toEqual([]); // one word alone: nothing to fuse

      c.toggle(deflated);
      expect(c.fusable().map((b) => b.token)).toEqual([blend]);
    });

    it('offers nothing for two words from different groups', () => {
      // "Calm, compassionate" is two feelings, both fully present — the picker
      // must not suggest they were really one thing in between.
      const { c } = setup(['Happy/Calm', 'Happy/Compassionate']);
      expect(c.fusable()).toEqual([]);
    });

    it('fuses the pair into one entry, and splits it back', () => {
      const { c } = setup([disheartened, deflated]);
      expect(c.count()).toBe(2);

      c.fuse(c.fusable()[0]);
      expect(c.selectedList()).toEqual([blend]);
      expect(c.count()).toBe(1); // one feeling, not two
      expect(c.label(blend)).toBe('Disheartened–Deflated');
      expect(c.fusable()).toEqual([]); // nothing left to fuse
      expect(c.blends().map((b) => b.token)).toEqual([blend]);

      c.split(c.blends()[0]);
      expect(c.selectedList()).toEqual([disheartened, deflated]);
      expect(c.blends()).toEqual([]);
    });

    it('lights both halves in the mosaic, at half strength', () => {
      const { c } = setup([blend]);
      // The blend lives in the set, not its halves — but the words must still show
      // as chosen, or you would lose sight of where your feeling sits.
      expect(c.isSelected(disheartened)).toBe(true);
      expect(c.isSelected(deflated)).toBe(true);
      expect(c.isBlended(disheartened)).toBe(true);
      expect(c.isBlended('Sad/Low')).toBe(false);
    });

    it('tapping half of a blend leaves the other word on its own', () => {
      const { c } = setup([blend]);
      c.toggle(disheartened);
      expect(c.selectedList()).toEqual([deflated]);
      expect(c.isSelected(disheartened)).toBe(false);
    });

    it('counts a blend once, under its single family', () => {
      const sad = EMOTION_WHEEL.find((core) => core.name === 'Sad')!;
      const { c } = setup([blend, 'Sad/Low']);
      expect(c.coreCount(sad)).toBe(2); // the blend is one entry, not two
    });

    it('closes with the blend as a single stored entry', () => {
      const { c, ref } = setup([disheartened, deflated]);
      c.fuse(c.fusable()[0]);
      c.done();
      expect(ref.close).toHaveBeenCalledWith([blend]);
    });
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
