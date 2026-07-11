import { describe, expect, it } from 'vitest';

import {
  EMOTION_LEAVES,
  EMOTION_WHEEL,
  emotionColor,
  emotionDesc,
  emotionLabel,
  emotionLeaf,
  emotionToken,
  searchEmotions,
} from './emotion-wheel';

describe('emotion-wheel', () => {
  it('has the full outer ring: every group holds at least the wheel’s two leaves', () => {
    // Roberts' wheel is exactly two per group; we extend it where a feeling has
    // no word at all, so a group may hold more — but never fewer.
    const groups = EMOTION_WHEEL.flatMap((c) => c.groups);
    expect(groups.length).toBe(42); // Roberts' 41, plus Bad › Agitated
    expect(groups.every((g) => g.leaves.length >= 2)).toBe(true);
    expect(EMOTION_LEAVES.length).toBe(groups.reduce((n, g) => n + g.leaves.length, 0));
  });

  it('keeps leaf names unique within a core', () => {
    // Tokens are `Core/Leaf`, so a word repeated inside one core would collide;
    // across cores it is fine (and deliberate).
    for (const core of EMOTION_WHEEL) {
      const leaves = core.groups.flatMap((g) => g.leaves.map((l) => l.name));
      expect(new Set(leaves).size, `duplicate leaf in ${core.name}`).toBe(leaves.length);
    }
  });

  it('every node — core, group, leaf — carries a non-empty gloss', () => {
    for (const core of EMOTION_WHEEL) {
      expect(core.desc.trim(), `core ${core.name}`).not.toBe('');
      for (const group of core.groups) {
        expect(group.desc.trim(), `group ${group.name}`).not.toBe('');
        for (const leaf of group.leaves) {
          expect(leaf.desc.trim(), `leaf ${leaf.name}`).not.toBe('');
        }
      }
    }
  });

  it('gives the two same-named leaves distinct glosses', () => {
    // "Overwhelmed" under Fearful vs Bad means subtly different things, and the
    // wheel spells that out rather than reusing one line.
    expect(emotionDesc('Fearful/Overwhelmed')).not.toBe(emotionDesc('Bad/Overwhelmed'));
    expect(emotionDesc('Fearful/Overwhelmed')).not.toBe('');
  });

  it('every leaf carries a unique qualified token', () => {
    const tokens = EMOTION_LEAVES.map((l) => l.token);
    expect(new Set(tokens).size).toBe(tokens.length); // no collisions
    expect(EMOTION_LEAVES.find((l) => l.leaf === 'Withdrawn')?.token).toBe('Angry/Withdrawn');
  });

  it('resolves a qualified token to its path and family colour', () => {
    expect(emotionLeaf('Angry/Withdrawn')).toEqual({
      token: 'Angry/Withdrawn',
      leaf: 'Withdrawn',
      desc: 'Pulled back and closed off from others.',
      secondary: 'Distant',
      core: 'Angry',
      color: 'angry',
    });
    expect(emotionColor('Angry/Withdrawn')).toBe('angry');
    expect(emotionLabel('Angry/Withdrawn')).toBe('Withdrawn');
    expect(emotionDesc('Angry/Withdrawn')).toBe('Pulled back and closed off from others.');
  });

  it('keeps a same-named leaf under two cores distinct', () => {
    // "Overwhelmed" is a leaf under both Fearful › Anxious and Bad › Stressed.
    // The qualified tokens resolve to different cores and colours — the crux of
    // "same name in different groups is NOT the same emotion".
    expect(emotionLeaf('Fearful/Overwhelmed')?.core).toBe('Fearful');
    expect(emotionLeaf('Bad/Overwhelmed')?.core).toBe('Bad');
    expect(emotionColor('Fearful/Overwhelmed')).not.toBe(emotionColor('Bad/Overwhelmed'));
  });

  it('resolves a legacy bare word to its first wheel occurrence', () => {
    // Pre-qualification check-ins stored a bare leaf. "Embarrassed" is under both
    // Sad › Hurt and Disgusted › Disapproving; Sad comes first, so that wins —
    // exactly as it displayed before tokens existed.
    expect(emotionLeaf('Embarrassed')?.core).toBe('Sad');
    expect(emotionToken('Embarrassed')).toBe('Sad/Embarrassed'); // upgrades on next save
  });

  it('canonicalises words: token passes through, bare upgrades, unknown kept', () => {
    expect(emotionToken('Bad/Overwhelmed')).toBe('Bad/Overwhelmed');
    expect(emotionToken('Withdrawn')).toBe('Angry/Withdrawn');
    expect(emotionToken('Flabbergasted')).toBe('Flabbergasted');
  });

  it('returns a neutral colour and verbatim label for an unknown word', () => {
    expect(emotionLeaf('Flabbergasted')).toBeNull();
    expect(emotionColor('Flabbergasted')).toBe('unknown');
    expect(emotionLabel('Flabbergasted')).toBe('Flabbergasted');
    expect(emotionDesc('Flabbergasted')).toBe('');
  });

  it('searches across leaf, secondary and core names', () => {
    expect(searchEmotions('with').map((l) => l.leaf)).toContain('Withdrawn');
    // "distant" is a secondary — its leaves surface.
    expect(searchEmotions('distant').map((l) => l.leaf)).toEqual(['Withdrawn', 'Numb']);
    expect(searchEmotions('  ')).toEqual([]);
  });
});
