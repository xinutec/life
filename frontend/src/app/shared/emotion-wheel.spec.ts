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
    // Roberts' 41, plus Agitated, Conflicted, Waiting (Bad); Caring, Elated,
    // Admiring, Desiring, Meaningful (Happy); Discouraged, Alienated (Sad); Flat,
    // Reflective (Neutral).
    expect(groups.length).toBe(53);
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
    // Two nodes now carry this name (Angry and Sad), so what matters is not
    // which comes first in the array but which a legacy bare word resolves to —
    // asserted in the pinning test below.
    expect(EMOTION_NODES.filter((n) => n.name === 'Withdrawn')).toHaveLength(2);
  });

  it('no longer resolves a bare word: every stored emotion is a token', () => {
    // Migration 0039 rewrote the last 15 bare values, so the first-occurrence
    // fallback is gone. This is what that buys: a word can now be added anywhere
    // in the wheel without re-pointing anything already recorded — the failure
    // that adding Sad/Withdrawn ahead of Angry/Withdrawn would have caused.
    expect(emotionNode('Withdrawn')).toBeNull();
    expect(emotionNode('Hopeful')).toBeNull();
    expect(emotionNode('Overwhelmed')).toBeNull();
    // Qualified tokens resolve, including both sides of a duplicated name.
    expect(emotionNode('Angry/Withdrawn')?.core).toBe('Angry');
    expect(emotionNode('Sad/Withdrawn')?.core).toBe('Sad');
    // An unresolvable word is still carried verbatim rather than dropped, so a
    // tag from a retired vocabulary is never silently lost.
    expect(emotionToken('Hopeful')).toBe('Hopeful');
    expect(emotionLabel('Hopeful')).toBe('Hopeful');
  });

  it('resolves a qualified token to its path and family colour', () => {
    expect(emotionNode('Angry/Withdrawn')).toEqual({
      token: 'Angry/Withdrawn',
      name: 'Withdrawn',
      desc: 'Pulled back to shut someone out — you are not giving them you.',
      kind: 'leaf',
      secondary: 'Distant',
      core: 'Angry',
      color: 'angry',
    });
    expect(emotionColor('Angry/Withdrawn')).toBe('angry');
    expect(emotionLabel('Angry/Withdrawn')).toBe('Withdrawn');
  });

  it('makes every twinned name say what its own core contributes', () => {
    // A duplicated name is only useful if the picker can tell you WHICH one you
    // are choosing, and the gloss is the only thing that can. This held for the
    // Sad copies of Withdrawn/Numb from the day they were added while the Angry
    // originals still read as being about nobody in particular — a half-made
    // distinction, which is worse than none. Asserted for every twin so the next
    // duplicate cannot land with an inherited gloss.
    const twins = EMOTION_NODES.filter(
      (n) => EMOTION_NODES.filter((m) => m.name === n.name).length > 1,
    );
    expect(twins.length).toBeGreaterThan(0);
    for (const node of twins) {
      const others = twins.filter((m) => m.name === node.name && m.token !== node.token);
      for (const other of others) {
        expect(node.desc, `${node.token} vs ${other.token}`).not.toBe(other.desc);
      }
    }
  });

  it('keeps a same-named leaf under two cores distinct', () => {
    // "Overwhelmed" is a leaf under both Fearful › Anxious and Bad › Stressed.
    // The qualified tokens resolve to different cores and colours — the crux of
    // "same name in different groups is NOT the same emotion".
    expect(emotionNode('Fearful/Overwhelmed')?.core).toBe('Fearful');
    expect(emotionNode('Bad/Overwhelmed')?.core).toBe('Bad');
    expect(emotionColor('Fearful/Overwhelmed')).not.toBe(emotionColor('Bad/Overwhelmed'));
  });

  it('keeps a duplicated name distinct per core', () => {
    // "Embarrassed" is under both Sad › Hurt and Disgusted › Disapproving, and
    // "Disappointed" is a Sad LEAF as well as a Disgusted GROUP. Each is its own
    // feeling with its own gloss; only the qualified token can tell them apart,
    // which is why storage uses tokens and nothing else.
    expect(emotionNode('Sad/Embarrassed')?.core).toBe('Sad');
    expect(emotionNode('Disgusted/Embarrassed')?.core).toBe('Disgusted');
    expect(emotionNode('Sad/Disappointed')).toMatchObject({ core: 'Sad', kind: 'leaf' });
    expect(emotionNode('Disgusted/Disappointed')).toMatchObject({
      core: 'Disgusted',
      kind: 'group',
    });
  });

  it('canonicalises words: a known token passes through, anything else is kept', () => {
    expect(emotionToken('Bad/Overwhelmed')).toBe('Bad/Overwhelmed');
    expect(emotionToken('Fearful/Anxious')).toBe('Fearful/Anxious'); // a group is a token too
    // Neither a bare word nor a retired one resolves, and neither is discarded.
    expect(emotionToken('Withdrawn')).toBe('Withdrawn');
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
