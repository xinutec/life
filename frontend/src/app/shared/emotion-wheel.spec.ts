import { describe, expect, it } from 'vitest';

import {
  EMOTION_LEAVES,
  EMOTION_WHEEL,
  emotionColor,
  emotionLeaf,
  searchEmotions,
} from './emotion-wheel';

describe('emotion-wheel', () => {
  it('has the full outer ring: every group holds two leaves', () => {
    const groups = EMOTION_WHEEL.flatMap((c) => c.groups);
    expect(groups.every((g) => g.leaves.length === 2)).toBe(true);
    expect(EMOTION_LEAVES.length).toBe(groups.length * 2);
    expect(EMOTION_LEAVES.length).toBe(82); // 41 secondary × 2
  });

  it('resolves a leaf to its path and family colour', () => {
    expect(emotionLeaf('Withdrawn')).toEqual({
      leaf: 'Withdrawn',
      secondary: 'Distant',
      core: 'Angry',
      color: 'angry',
    });
    expect(emotionColor('Withdrawn')).toBe('angry');
  });

  it('resolves a duplicated leaf to its first wheel occurrence', () => {
    // "Embarrassed" is under both Sad › Hurt and Disgusted › Disapproving; Sad
    // comes first in the wheel, so that path wins.
    expect(emotionLeaf('Embarrassed')?.core).toBe('Sad');
  });

  it('returns a neutral colour for an unknown word', () => {
    expect(emotionLeaf('Flabbergasted')).toBeNull();
    expect(emotionColor('Flabbergasted')).toBe('unknown');
  });

  it('searches across leaf, secondary and core names', () => {
    expect(searchEmotions('with').map((l) => l.leaf)).toContain('Withdrawn');
    // "distant" is a secondary — its leaves surface.
    expect(searchEmotions('distant').map((l) => l.leaf)).toEqual(['Withdrawn', 'Numb']);
    expect(searchEmotions('  ')).toEqual([]);
  });
});
