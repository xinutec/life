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

  it('pins a legacy bare word to the core it displayed as, not to wheel order', () => {
    // These three were duplicated into an EARLIER core than the one a check-in
    // was saved against. Left to first-occurrence, an old bare 'Withdrawn' would
    // silently become Sad — re-colouring history and asserting a feeling that
    // was never recorded. The pin is what stops position deciding this.
    expect(emotionToken('Withdrawn')).toBe('Angry/Withdrawn');
    expect(emotionToken('Numb')).toBe('Angry/Numb');
    expect(emotionToken('Hesitant')).toBe('Disgusted/Hesitant');
    // Ambiguous before this revision; pinned where position had already put them.
    expect(emotionToken('Embarrassed')).toBe('Sad/Embarrassed');
    expect(emotionToken('Inferior')).toBe('Sad/Inferior');
    expect(emotionToken('Overwhelmed')).toBe('Fearful/Overwhelmed');
    // The new duplicates are still reachable — by their qualified token.
    expect(emotionNode('Sad/Withdrawn')?.core).toBe('Sad');
    expect(emotionNode('Fearful/Hesitant')?.core).toBe('Fearful');
  });

  it('resolves every bare word still stored in production', () => {
    // The exact set found in the wellbeing table (2026-07-25, 15 values across
    // 10 words) and what each must become. Migration 0039 rewrites them to these
    // tokens, so this is the authority the SQL was written from — if the wheel
    // ever moves one of these, this fails rather than the history quietly
    // re-pointing.
    const stored: Readonly<Record<string, string>> = {
      Hopeful: 'Happy/Hopeful',
      Worried: 'Fearful/Worried',
      Thankful: 'Happy/Thankful',
      Isolated: 'Sad/Isolated',
      Overwhelmed: 'Fearful/Overwhelmed', // ambiguous: also Bad/Overwhelmed
      Disappointed: 'Sad/Disappointed', // ambiguous: also the Disgusted GROUP
      Annoyed: 'Angry/Annoyed',
      Loving: 'Happy/Loving',
      Sleepy: 'Bad/Sleepy',
      Inspired: 'Happy/Inspired',
    };
    for (const [bare, token] of Object.entries(stored)) {
      expect(emotionToken(bare), `bare "${bare}"`).toBe(token);
      expect(emotionNode(token), `token "${token}"`).not.toBeNull();
    }
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
