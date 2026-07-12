import { describe, expect, it } from 'vitest';

import {
  EMOTION_NODES,
  EMOTION_WHEEL,
  blendToken,
  canBlend,
  emotionBlend,
  emotionColor,
  emotionCore,
  emotionDesc,
  emotionLabel,
  emotionNode,
  emotionToken,
  searchEmotions,
} from './emotion-wheel';

/** A node by token, for the blend tests. */
function node(token: string) {
  return emotionNode(token)!;
}

describe('emotion-wheel', () => {
  it('has the full outer ring: every group holds at least the wheel’s two leaves', () => {
    // Roberts' wheel is exactly two per group; we extend it where a feeling has
    // no word at all, so a group may hold more — but never fewer.
    const groups = EMOTION_WHEEL.flatMap((c) => c.groups);
    // Roberts' 41, plus Agitated, Conflicted, Waiting (Bad); Caring, Elated
    // (Happy); Discouraged (Sad).
    expect(groups.length).toBe(47);
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

  describe('blends — one feeling between two words', () => {
    const disheartened = 'Sad/Disheartened';
    const deflated = 'Sad/Deflated';
    const blend = `${disheartened}+${deflated}`;

    it('blends two leaves of one group, in canonical order either way round', () => {
      expect(blendToken(node(disheartened), node(deflated))).toBe(blend);
      // Order-free: the feeling is the same whichever word you tapped first, so
      // it must not be able to arrive in the record as two different tokens.
      expect(blendToken(node(deflated), node(disheartened))).toBe(blend);
      expect(emotionToken(`${deflated}+${disheartened}`)).toBe(blend);
    });

    it('refuses to blend across groups, across cores, or with a group word', () => {
      // Different group, same core: "empty" and "disheartened" are two feelings,
      // and selecting both already says so.
      expect(canBlend(node(disheartened), node('Sad/Empty'))).toBe(false);
      // Different core: there is no midpoint between families, and the blend would
      // have no single colour to fly.
      expect(canBlend(node(disheartened), node('Fearful/Worried'))).toBe(false);
      // A group already contains its leaves — there is no gap between Frustrated
      // and Annoyed to sit in.
      expect(canBlend(node('Angry/Frustrated'), node('Angry/Annoyed'))).toBe(false);
      // And nothing blends with itself.
      expect(canBlend(node(disheartened), node(disheartened))).toBe(false);
      expect(blendToken(node(disheartened), node('Sad/Empty'))).toBeNull();
    });

    it('resolves a blend to one entry with one family, colour, name and gloss', () => {
      const b = emotionBlend(blend)!;
      expect(b.a.name).toBe('Disheartened');
      expect(b.b.name).toBe('Deflated');
      expect(b.core).toBe('Sad');
      expect(b.color).toBe('sad');
      expect(emotionLabel(blend)).toBe('Disheartened–Deflated');
      expect(emotionColor(blend)).toBe('sad'); // one family: history stays intact
      expect(emotionCore(blend)).toBe('Sad');
      expect(emotionDesc(blend)).toContain('Neither word alone');
      expect(emotionDesc(blend)).toContain('The wind has gone out of you.');
    });

    it('is not a node, and a node is not a blend', () => {
      // The two are distinct kinds of entry: selecting both words says both
      // feelings were fully present; a blend says there was one, in the gap.
      expect(emotionNode(blend)).toBeNull();
      expect(emotionBlend(disheartened)).toBeNull();
    });

    it('accepts legacy bare halves and canonicalises them', () => {
      expect(emotionToken('Disheartened+Deflated')).toBe(blend);
    });

    it('keeps an illegal or unknown pair verbatim rather than reinterpreting it', () => {
      // A hand-edited or retired value must never be silently re-pointed at a
      // feeling the person did not choose.
      const illegal = 'Sad/Disheartened+Fearful/Worried';
      expect(emotionBlend(illegal)).toBeNull();
      expect(emotionToken(illegal)).toBe(illegal);
      expect(emotionLabel(illegal)).toBe(illegal);
      expect(emotionColor(illegal)).toBe('unknown');
      expect(emotionBlend('Sad/Low+Flabbergasted')).toBeNull();
      expect(emotionBlend('a+b+c')).toBeNull();
    });

    it('holds no emotion name containing the blend delimiter', () => {
      // '+' is the delimiter; a name containing one would make a token ambiguous.
      for (const n of EMOTION_NODES) expect(n.name).not.toContain('+');
    });
  });

  it('searches across node, secondary and core names', () => {
    expect(searchEmotions('with').map((n) => n.name)).toContain('Withdrawn');
    // "distant" is a secondary — it and its leaves surface, the group first.
    expect(searchEmotions('distant').map((n) => n.name)).toEqual(['Distant', 'Withdrawn', 'Numb']);
    expect(searchEmotions('  ')).toEqual([]);
  });
});
