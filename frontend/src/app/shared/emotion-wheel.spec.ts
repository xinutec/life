import { describe, expect, it } from 'vitest';

import {
  EMOTION_NODES,
  EMOTION_WHEEL,
  emotionColor,
  emotionDesc,
  emotionLabel,
  emotionNode,
  emotionToken,
  searchEmotions,
} from './emotion-wheel';

describe('emotion-wheel', () => {
  it('has the full outer ring: every group holds at least the wheel’s two leaves', () => {
    // Roberts' wheel is exactly two per group; we extend it where a feeling has
    // no word at all, so a group may hold more — but never fewer.
    const groups = EMOTION_WHEEL.flatMap((c) => c.groups);
    // Roberts' 41, plus Agitated, Conflicted, Waiting (Bad); Caring, Elated (Happy).
    expect(groups.length).toBe(46);
    expect(groups.every((g) => g.leaves.length >= 2)).toBe(true);
  });

  it('makes both rings selectable: every group and every leaf is a node', () => {
    const groups = EMOTION_WHEEL.flatMap((c) => c.groups);
    const leaves = groups.reduce((n, g) => n + g.leaves.length, 0);
    expect(EMOTION_NODES.filter((n) => n.kind === 'group')).toHaveLength(groups.length);
    expect(EMOTION_NODES.filter((n) => n.kind === 'leaf')).toHaveLength(leaves);
    // A group is a legitimate answer: "frustrated" is often the whole truth.
    expect(emotionNode('Angry/Frustrated')).toEqual({
      token: 'Angry/Frustrated',
      name: 'Frustrated',
      desc: 'Blocked from what you want.',
      kind: 'group',
      secondary: 'Frustrated',
      core: 'Angry',
      color: 'angry',
    });
  });

  it('keeps every name unique within a core, across both rings', () => {
    // Tokens are `Core/Name`, so a word repeated inside one core — whether as a
    // group or a leaf — would collide. Across cores it is fine (and deliberate).
    for (const core of EMOTION_WHEEL) {
      const names = core.groups.flatMap((g) => [g.name, ...g.leaves.map((l) => l.name)]);
      expect(new Set(names).size, `duplicate name in ${core.name}`).toBe(names.length);
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

  it('every node carries a unique qualified token', () => {
    const tokens = EMOTION_NODES.map((n) => n.token);
    expect(new Set(tokens).size).toBe(tokens.length); // no collisions
    expect(EMOTION_NODES.find((n) => n.name === 'Withdrawn')?.token).toBe('Angry/Withdrawn');
  });

  it('resolves a qualified token to its path and family colour', () => {
    expect(emotionNode('Angry/Withdrawn')).toEqual({
      token: 'Angry/Withdrawn',
      name: 'Withdrawn',
      desc: 'Pulled back and closed off from others.',
      kind: 'leaf',
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
    expect(emotionNode('Fearful/Overwhelmed')?.core).toBe('Fearful');
    expect(emotionNode('Bad/Overwhelmed')?.core).toBe('Bad');
    expect(emotionColor('Fearful/Overwhelmed')).not.toBe(emotionColor('Bad/Overwhelmed'));
  });

  it('resolves a legacy bare word to its first wheel occurrence', () => {
    // Pre-qualification check-ins stored a bare word. "Embarrassed" is under both
    // Sad › Hurt and Disgusted › Disapproving; Sad comes first, so that wins —
    // exactly as it displayed before tokens existed.
    expect(emotionNode('Embarrassed')?.core).toBe('Sad');
    expect(emotionToken('Embarrassed')).toBe('Sad/Embarrassed'); // upgrades on next save
  });

  it('prefers a leaf over a group when a legacy bare word matches both', () => {
    // "Disappointed" is a Sad leaf and (under another core) a Disgusted group.
    // An old bare value must still resolve to the leaf it always displayed as —
    // making groups selectable must not re-point historical check-ins.
    expect(emotionNode('Disappointed')).toMatchObject({ core: 'Sad', kind: 'leaf' });
    expect(emotionToken('Disappointed')).toBe('Sad/Disappointed');
  });

  it('canonicalises words: token passes through, bare upgrades, unknown kept', () => {
    expect(emotionToken('Bad/Overwhelmed')).toBe('Bad/Overwhelmed');
    expect(emotionToken('Withdrawn')).toBe('Angry/Withdrawn');
    expect(emotionToken('Anxious')).toBe('Fearful/Anxious'); // a group word, now resolvable
    expect(emotionToken('Flabbergasted')).toBe('Flabbergasted');
  });

  it('returns a neutral colour and verbatim label for an unknown word', () => {
    expect(emotionNode('Flabbergasted')).toBeNull();
    expect(emotionColor('Flabbergasted')).toBe('unknown');
    expect(emotionLabel('Flabbergasted')).toBe('Flabbergasted');
    expect(emotionDesc('Flabbergasted')).toBe('');
  });

  it('searches across node, secondary and core names', () => {
    expect(searchEmotions('with').map((n) => n.name)).toContain('Withdrawn');
    // "distant" is a secondary — it and its leaves surface, the group first.
    expect(searchEmotions('distant').map((n) => n.name)).toEqual(['Distant', 'Withdrawn', 'Numb']);
    expect(searchEmotions('  ')).toEqual([]);
  });
});
