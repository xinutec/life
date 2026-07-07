/** The Geoffrey Roberts "Feelings Wheel": a fixed, three-tier emotional
 *  vocabulary — 7 core emotions, each with a ring of secondary feelings, each
 *  with two fine-grained tertiary leaves (~130 in all). Held as static data (no
 *  backend): a check-in records a set of *leaf* words, and their path back up to
 *  the core is derived here for display and colour.
 *
 *  Leaf words are the stored identity. A few leaves ("Embarrassed",
 *  "Disappointed", "Inferior", "Overwhelmed") appear under two cores in the
 *  canonical wheel — [[emotionPath]] resolves to the first, which is enough for
 *  labelling; the stored word is unambiguous as a tag. */

export interface EmotionGroup {
  /** The middle-ring (secondary) feeling. */
  name: string;
  /** The outer-ring (tertiary) leaves under it. */
  leaves: readonly string[];
}

export interface EmotionCore {
  /** The centre-ring core emotion. */
  name: string;
  /** Colour key → `--emo-<color>` token (see emotion-picker.scss), matching the
   *  wheel's hues so a chip stays recognisable by family. */
  color: string;
  groups: readonly EmotionGroup[];
}

export const EMOTION_WHEEL: readonly EmotionCore[] = [
  {
    name: 'Happy',
    color: 'happy',
    groups: [
      { name: 'Playful', leaves: ['Aroused', 'Cheeky'] },
      { name: 'Content', leaves: ['Free', 'Joyful'] },
      { name: 'Interested', leaves: ['Curious', 'Inquisitive'] },
      { name: 'Proud', leaves: ['Successful', 'Confident'] },
      { name: 'Accepted', leaves: ['Respected', 'Valued'] },
      { name: 'Powerful', leaves: ['Courageous', 'Creative'] },
      { name: 'Peaceful', leaves: ['Loving', 'Thankful'] },
      { name: 'Trusting', leaves: ['Sensitive', 'Intimate'] },
      { name: 'Optimistic', leaves: ['Hopeful', 'Inspired'] },
    ],
  },
  {
    name: 'Sad',
    color: 'sad',
    groups: [
      { name: 'Lonely', leaves: ['Isolated', 'Abandoned'] },
      { name: 'Vulnerable', leaves: ['Victimised', 'Fragile'] },
      { name: 'Despair', leaves: ['Grief', 'Powerless'] },
      { name: 'Guilty', leaves: ['Ashamed', 'Remorseful'] },
      { name: 'Depressed', leaves: ['Inferior', 'Empty'] },
      { name: 'Hurt', leaves: ['Embarrassed', 'Disappointed'] },
    ],
  },
  {
    name: 'Disgusted',
    color: 'disgusted',
    groups: [
      { name: 'Disapproving', leaves: ['Judgemental', 'Embarrassed'] },
      { name: 'Disappointed', leaves: ['Appalled', 'Revolted'] },
      { name: 'Awful', leaves: ['Nauseated', 'Detestable'] },
      { name: 'Repelled', leaves: ['Horrified', 'Hesitant'] },
    ],
  },
  {
    name: 'Angry',
    color: 'angry',
    groups: [
      { name: 'Let down', leaves: ['Betrayed', 'Resentful'] },
      { name: 'Humiliated', leaves: ['Disrespected', 'Ridiculed'] },
      { name: 'Bitter', leaves: ['Indignant', 'Violated'] },
      { name: 'Mad', leaves: ['Furious', 'Jealous'] },
      { name: 'Aggressive', leaves: ['Provoked', 'Hostile'] },
      { name: 'Frustrated', leaves: ['Infuriated', 'Annoyed'] },
      { name: 'Distant', leaves: ['Withdrawn', 'Numb'] },
      { name: 'Critical', leaves: ['Sceptical', 'Dismissive'] },
    ],
  },
  {
    name: 'Fearful',
    color: 'fearful',
    groups: [
      { name: 'Scared', leaves: ['Helpless', 'Frightened'] },
      { name: 'Anxious', leaves: ['Overwhelmed', 'Worried'] },
      { name: 'Insecure', leaves: ['Inadequate', 'Inferior'] },
      { name: 'Weak', leaves: ['Worthless', 'Insignificant'] },
      { name: 'Rejected', leaves: ['Excluded', 'Persecuted'] },
      { name: 'Threatened', leaves: ['Nervous', 'Exposed'] },
    ],
  },
  {
    name: 'Bad',
    color: 'bad',
    groups: [
      { name: 'Bored', leaves: ['Indifferent', 'Apathetic'] },
      { name: 'Busy', leaves: ['Pressured', 'Rushed'] },
      { name: 'Stressed', leaves: ['Overwhelmed', 'Out of control'] },
      { name: 'Tired', leaves: ['Sleepy', 'Unfocused'] },
    ],
  },
  {
    name: 'Surprised',
    color: 'surprised',
    groups: [
      { name: 'Startled', leaves: ['Shocked', 'Dismayed'] },
      { name: 'Confused', leaves: ['Disillusioned', 'Perplexed'] },
      { name: 'Amazed', leaves: ['Astonished', 'In awe'] },
      { name: 'Excited', leaves: ['Eager', 'Energetic'] },
    ],
  },
];

/** One outer-ring leaf, flattened with its path up to the core (for search and
 *  chip labelling). */
export interface EmotionLeaf {
  leaf: string;
  secondary: string;
  core: string;
  color: string;
}

/** Every leaf, in wheel order. First occurrence of a duplicated word wins for
 *  path resolution. */
export const EMOTION_LEAVES: readonly EmotionLeaf[] = EMOTION_WHEEL.flatMap((core) =>
  core.groups.flatMap((group) =>
    group.leaves.map((leaf) => ({ leaf, secondary: group.name, core: core.name, color: core.color })),
  ),
);

const LEAF_BY_NAME = new Map<string, EmotionLeaf>();
for (const l of EMOTION_LEAVES) if (!LEAF_BY_NAME.has(l.leaf)) LEAF_BY_NAME.set(l.leaf, l);

/** Resolve a stored leaf word to its wheel entry (path + colour), or null if it
 *  isn't in the vocabulary (e.g. a word retired from a later wheel revision). */
export function emotionLeaf(word: string): EmotionLeaf | null {
  return LEAF_BY_NAME.get(word) ?? null;
}

/** The colour key for a stored word — the family it belongs to, or a neutral
 *  fallback for an unknown word. */
export function emotionColor(word: string): string {
  return emotionLeaf(word)?.color ?? 'unknown';
}

/** Case-insensitive substring search across leaf, secondary and core names, so
 *  typing "with" finds Withdrawn and typing "ang" surfaces the Angry family. */
export function searchEmotions(query: string): readonly EmotionLeaf[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return EMOTION_LEAVES.filter(
    (l) =>
      l.leaf.toLowerCase().includes(q) ||
      l.secondary.toLowerCase().includes(q) ||
      l.core.toLowerCase().includes(q),
  );
}
