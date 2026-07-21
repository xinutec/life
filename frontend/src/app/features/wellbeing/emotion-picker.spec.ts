import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { Observable, of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { LifeApi } from '../../life-api';
import type { SuggestEmotionsRequest, SuggestEmotionsResponse } from '../../models';
import { EMOTION_WHEEL } from '../../shared/emotion-wheel';
import { EmotionPicker } from './emotion-picker';

describe('EmotionPicker', () => {
  function setup(
    selected: string[],
    opts: {
      note?: string;
      suggestions?: string[];
      stale?: boolean;
      pending?: boolean;
      thinkingSecs?: number;
    } = {},
  ) {
    const ref = { close: vi.fn() };
    const suggestEmotions = vi.fn<
      (body: SuggestEmotionsRequest) => Observable<SuggestEmotionsResponse>
    >(() =>
      of({
        suggestions: opts.suggestions ?? [],
        stale: opts.stale ?? false,
        pending: opts.pending ?? false,
        thinkingSecs: opts.thinkingSecs ?? null,
      }),
    );
    TestBed.configureTestingModule({
      imports: [EmotionPicker],
      providers: [
        { provide: MatDialogRef, useValue: ref },
        {
          provide: MAT_DIALOG_DATA,
          useValue: { selected, ulid: '01J0EMOTIONPICKERSPEC0000', note: opts.note },
        },
        { provide: LifeApi, useValue: { suggestEmotions } },
      ],
    });
    const fixture = TestBed.createComponent(EmotionPicker);
    return { c: fixture.componentInstance, fixture, ref, suggestEmotions };
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

  it('asks for suggestions when opened with a note, and resolves the nodes', () => {
    const { c, suggestEmotions } = setup([], {
      note: 'A tough day, feeling down',
      suggestions: ['Sad/Low', 'Sad/Empty'],
    });
    // The whole wheel was offered as candidates, the note and the check-in it
    // belongs to passed through (the latter is the cache key).
    expect(suggestEmotions).toHaveBeenCalledOnce();
    const body = suggestEmotions.mock.calls[0][0];
    expect(body.note).toBe('A tough day, feeling down');
    expect(body.ulid).toBe('01J0EMOTIONPICKERSPEC0000');
    expect(body.candidates.length).toBeGreaterThan(100);
    // Returned tokens resolve to full nodes (name + colour) for rendering.
    expect(c.thinking()).toBe(false);
    expect(c.suggestions().map((n) => n.token)).toEqual(['Sad/Low', 'Sad/Empty']);
    expect(c.suggestions()[0].name).toBe('Low');
  });

  it('drops a suggested token the vocabulary no longer knows', () => {
    const { c } = setup([], { note: 'x', suggestions: ['Sad/Low', 'Sad/Retired-word'] });
    expect(c.suggestions().map((n) => n.token)).toEqual(['Sad/Low']);
  });

  it('does not call the suggest API when there is no note', () => {
    const { c, suggestEmotions } = setup(['Angry/Numb']);
    expect(suggestEmotions).not.toHaveBeenCalled();
    expect(c.thinking()).toBe(false);
    expect(c.suggestions()).toEqual([]);
  });

  it('keeps the previous wording\u2019s suggestions on screen while rereading', () => {
    const { c, fixture } = setup([], {
      note: 'reworded',
      suggestions: ['Sad/Low'],
      stale: true,
      pending: true,
      thinkingSecs: 7,
    });
    // Shown, marked as belonging to the earlier note, with the server's clock.
    expect(c.suggestions().map((n) => n.token)).toEqual(['Sad/Low']);
    expect(c.stale()).toBe(true);
    expect(c.thinking()).toBe(true);
    expect(c.thinkingSecs()).toBe(7);
    fixture.destroy(); // stops the poll this test started
  });

  it('does not claim to be thinking when nothing is computing', () => {
    // No worker running: the server says so, and the picker stays silent rather
    // than showing a spinner nothing is behind.
    const { c, fixture } = setup([], { note: 'a note', suggestions: [] });
    expect(c.thinking()).toBe(false);
    expect(c.suggestions()).toEqual([]);
    fixture.destroy();
  });
});
