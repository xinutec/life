/** A three-tier emotional vocabulary — 7 core emotions, each with a ring of
 *  secondary feelings, each with fine-grained tertiary leaves. Held as static
 *  data (no backend): a check-in records a set of emotions, and their path back
 *  up to the core is derived here for display and colour. Every node carries a
 *  brief plain-English gloss of what it means, so the picker can explain each
 *  feeling.
 *
 *  It started as the Geoffrey Roberts "Feelings Wheel" (a strict two leaves per
 *  group) and is being extended where that wheel leaves a real feeling with no
 *  word at all — so groups may now hold more than two leaves.
 *
 *  Identity is the *qualified* token `Core/Leaf` (see [[emotionToken]]), not the
 *  bare leaf word — because a few leaves ("Embarrassed", "Inferior",
 *  "Overwhelmed") sit under two different cores, and a bare word can't tell them
 *  apart (same name in different groups is NOT the same emotion, and their
 *  glosses differ accordingly). The `/` delimiter is safe: no core or leaf name
 *  contains a slash.
 *
 *  Back-compat: check-ins saved before qualification stored a bare leaf. Those
 *  still resolve, via a first-occurrence fallback, to exactly the core they
 *  always displayed as — so nothing regresses and we never invent which core an
 *  old ambiguous word "really" meant. */

/** One outer-ring (tertiary) leaf: the word plus a brief gloss. */
export interface EmotionLeafDef {
  name: string;
  desc: string;
}

export interface EmotionGroup {
  /** The middle-ring (secondary) feeling. */
  name: string;
  /** A brief gloss of the group. */
  desc: string;
  /** The outer-ring (tertiary) leaves under it. */
  leaves: readonly EmotionLeafDef[];
}

export interface EmotionCore {
  /** The centre-ring core emotion. */
  name: string;
  /** Colour key → `--emo-<color>` token (see emotion-picker.scss), matching the
   *  wheel's hues so a chip stays recognisable by family. */
  color: string;
  /** A brief gloss of the core family. */
  desc: string;
  groups: readonly EmotionGroup[];
}

export const EMOTION_WHEEL: readonly EmotionCore[] = [
  {
    name: 'Happy',
    color: 'happy',
    desc: 'Positive wellbeing — joy, contentment, and warmth toward life.',
    groups: [
      {
        name: 'Playful',
        desc: 'In high spirits and up for fun, teasing, or mischief.',
        leaves: [
          { name: 'Aroused', desc: 'Stimulated and keenly alert; energised in the moment.' },
          { name: 'Cheeky', desc: 'Playfully bold or irreverent; teasing without meaning harm.' },
        ],
      },
      {
        name: 'Content',
        desc: 'Quietly satisfied and at ease with how things are.',
        leaves: [
          { name: 'Free', desc: 'Unburdened and able to be yourself, without constraint.' },
          { name: 'Joyful', desc: 'Bright, buoyant gladness that lifts you.' },
        ],
      },
      {
        name: 'Interested',
        desc: 'Curiosity that pulls your attention toward something.',
        leaves: [
          { name: 'Curious', desc: 'Eager to explore, learn, or find out more.' },
          { name: 'Inquisitive', desc: 'Actively questioning and probing for understanding.' },
        ],
      },
      {
        name: 'Proud',
        desc: "Warm satisfaction in yourself or something you've done.",
        leaves: [
          { name: 'Successful', desc: 'Pleased at having achieved or accomplished something.' },
          { name: 'Confident', desc: 'Sure of your own worth and abilities.' },
        ],
      },
      {
        name: 'Accepted',
        desc: 'Feeling you belong and are welcome as you are.',
        leaves: [
          { name: 'Respected', desc: 'Held in regard; your worth acknowledged by others.' },
          { name: 'Valued', desc: 'Treated as important and worth caring about.' },
        ],
      },
      {
        name: 'Powerful',
        desc: 'A sense of capability and agency.',
        leaves: [
          { name: 'Courageous', desc: 'Willing to face difficulty or fear with resolve.' },
          { name: 'Creative', desc: 'Inventive and generative; full of ideas.' },
        ],
      },
      {
        name: 'Peaceful',
        desc: 'Calm, settled contentment.',
        leaves: [
          { name: 'Loving', desc: 'Warm and tender affection toward others.' },
          { name: 'Thankful', desc: 'Grateful and appreciative for what you have.' },
          {
            name: 'Relieved',
            desc: "The tension drops away — the thing you dreaded didn't happen.",
          },
        ],
      },
      {
        name: 'Trusting',
        desc: 'Feeling safe enough to open up to others.',
        leaves: [
          { name: 'Sensitive', desc: "Open and finely attuned to feeling, yours and others'." },
          { name: 'Intimate', desc: 'Emotionally close and connected to someone.' },
        ],
      },
      {
        name: 'Optimistic',
        desc: 'Expecting things to turn out well.',
        leaves: [
          { name: 'Hopeful', desc: 'Expecting good things and looking forward.' },
          { name: 'Inspired', desc: 'Moved and uplifted to act or create.' },
        ],
      },
    ],
  },
  {
    name: 'Sad',
    color: 'sad',
    desc: 'Low, heavy feelings of loss, sorrow, or discouragement.',
    groups: [
      {
        name: 'Lonely',
        desc: 'Painfully apart from others; lacking connection.',
        leaves: [
          { name: 'Isolated', desc: 'Cut off from others, with no one near.' },
          { name: 'Abandoned', desc: 'Left alone by those you counted on.' },
        ],
      },
      {
        name: 'Vulnerable',
        desc: 'Exposed and easily hurt.',
        leaves: [
          { name: 'Victimised', desc: 'Wronged or taken advantage of by another.' },
          { name: 'Fragile', desc: 'Easily broken or overwhelmed; delicate right now.' },
        ],
      },
      {
        name: 'Despair',
        desc: 'Deep, hopeless sorrow.',
        leaves: [
          { name: 'Grief', desc: 'Deep sorrow, especially at a loss.' },
          { name: 'Powerless', desc: "Unable to change or influence what's happening." },
        ],
      },
      {
        name: 'Guilty',
        desc: 'Troubled by having done wrong.',
        leaves: [
          { name: 'Ashamed', desc: 'Painfully aware of having fallen short.' },
          { name: 'Remorseful', desc: 'Sorry and regretful for something you did.' },
        ],
      },
      {
        name: 'Depressed',
        desc: 'Flattened, joyless low mood.',
        leaves: [
          { name: 'Inferior', desc: 'Feeling lesser or not good enough next to others.' },
          { name: 'Empty', desc: 'Hollow and without feeling or meaning.' },
        ],
      },
      {
        name: 'Hurt',
        desc: 'Emotionally wounded by another.',
        leaves: [
          { name: 'Embarrassed', desc: 'Self-conscious and awkward after exposure or a slip.' },
          { name: 'Disappointed', desc: "Let down when hopes weren't met." },
        ],
      },
    ],
  },
  {
    name: 'Disgusted',
    color: 'disgusted',
    desc: 'Repulsion or strong distaste toward something offensive.',
    groups: [
      {
        name: 'Disapproving',
        desc: 'Judging something as wrong or unacceptable.',
        leaves: [
          { name: 'Judgemental', desc: "Harshly critical of others' choices or worth." },
          { name: 'Embarrassed', desc: 'Uncomfortably self-conscious, wanting to shrink from view.' },
        ],
      },
      {
        name: 'Disappointed',
        desc: 'Offended by something falling below standard.',
        leaves: [
          { name: 'Appalled', desc: 'Shocked and dismayed by something offensive.' },
          { name: 'Revolted', desc: 'Filled with strong disgust; recoiling from it.' },
        ],
      },
      {
        name: 'Awful',
        desc: 'A sick, repelled sense of something terrible.',
        leaves: [
          { name: 'Nauseated', desc: 'Sickened, as if turned in the stomach.' },
          { name: 'Detestable', desc: 'Loathing something as thoroughly hateful.' },
        ],
      },
      {
        name: 'Repelled',
        desc: 'Pushed away by something distasteful.',
        leaves: [
          { name: 'Horrified', desc: 'Struck with shock and dread at something dreadful.' },
          { name: 'Hesitant', desc: 'Holding back, wary or reluctant to engage.' },
        ],
      },
    ],
  },
  {
    name: 'Angry',
    color: 'angry',
    desc: 'Hot displeasure at being wronged, blocked, or violated.',
    groups: [
      {
        name: 'Let down',
        desc: 'Failed by someone you relied on.',
        leaves: [
          { name: 'Betrayed', desc: 'Wounded by a broken trust or loyalty.' },
          { name: 'Resentful', desc: 'Bitter over being treated unfairly.' },
        ],
      },
      {
        name: 'Humiliated',
        desc: "Shamed and lowered in others' eyes.",
        leaves: [
          { name: 'Disrespected', desc: 'Treated without the regard you deserve.' },
          { name: 'Ridiculed', desc: 'Mocked or made fun of.' },
        ],
      },
      {
        name: 'Bitter',
        desc: 'Sour, lasting anger over a wrong.',
        leaves: [
          { name: 'Indignant', desc: 'Angered by unfairness or injustice.' },
          { name: 'Violated', desc: 'Deeply wronged; your boundaries breached.' },
        ],
      },
      {
        name: 'Mad',
        desc: 'Hot, active anger.',
        leaves: [
          { name: 'Furious', desc: 'Intensely, fiercely angry.' },
          { name: 'Jealous', desc: 'Threatened by a rival; resentful of what they have.' },
        ],
      },
      {
        name: 'Aggressive',
        desc: 'Anger pushing outward toward confrontation.',
        leaves: [
          { name: 'Provoked', desc: 'Stirred to anger by something deliberate.' },
          { name: 'Hostile', desc: 'Antagonistic and ready to attack or oppose.' },
        ],
      },
      {
        name: 'Frustrated',
        desc: 'Blocked from what you want.',
        leaves: [
          { name: 'Infuriated', desc: 'Maddened; anger boiling over.' },
          { name: 'Annoyed', desc: 'Mildly irritated or bothered.' },
        ],
      },
      {
        name: 'Distant',
        desc: 'Anger that withdraws and shuts down.',
        leaves: [
          { name: 'Withdrawn', desc: 'Pulled back and closed off from others.' },
          { name: 'Numb', desc: 'Deadened; feeling little or nothing.' },
        ],
      },
      {
        name: 'Critical',
        desc: 'A fault-finding, dismissive stance.',
        leaves: [
          { name: 'Sceptical', desc: 'Doubting; unconvinced and questioning.' },
          { name: 'Dismissive', desc: 'Treating things as unworthy of attention.' },
        ],
      },
    ],
  },
  {
    name: 'Fearful',
    color: 'fearful',
    desc: 'A sense of threat or danger, and the urge to protect yourself.',
    groups: [
      {
        name: 'Scared',
        desc: 'Frightened by a present threat.',
        leaves: [
          { name: 'Helpless', desc: 'Unable to protect yourself or cope.' },
          { name: 'Frightened', desc: 'Afraid in the face of danger.' },
        ],
      },
      {
        name: 'Anxious',
        desc: 'Uneasy dread about what might happen.',
        leaves: [
          { name: 'Overwhelmed', desc: 'Swamped; more coming at you than you can take in.' },
          { name: 'Worried', desc: 'Anxiously turning over what could go wrong.' },
        ],
      },
      {
        name: 'Insecure',
        desc: 'Unsure of your footing or worth.',
        leaves: [
          { name: 'Inadequate', desc: 'Feeling not capable or good enough.' },
          { name: 'Inferior', desc: 'Feeling lesser than those around you.' },
        ],
      },
      {
        name: 'Weak',
        desc: 'Without strength or standing.',
        leaves: [
          { name: 'Worthless', desc: 'Feeling of no value at all.' },
          { name: 'Insignificant', desc: 'Small and unimportant; easily overlooked.' },
        ],
      },
      {
        name: 'Rejected',
        desc: 'Pushed out or unwanted.',
        leaves: [
          { name: 'Excluded', desc: 'Left out and kept apart from the group.' },
          { name: 'Persecuted', desc: 'Singled out for unfair, hostile treatment.' },
        ],
      },
      {
        name: 'Threatened',
        desc: 'Sensing danger to you or yours.',
        leaves: [
          { name: 'Nervous', desc: 'Jittery and on edge with apprehension.' },
          { name: 'Exposed', desc: 'Unprotected and open to harm.' },
        ],
      },
    ],
  },
  {
    name: 'Bad',
    color: 'bad',
    desc: 'Depleted, off-colour states — drained, pressured, or run down.',
    groups: [
      {
        name: 'Bored',
        desc: 'Understimulated and disengaged.',
        leaves: [
          { name: 'Indifferent', desc: 'Uninterested; unmoved either way.' },
          { name: 'Apathetic', desc: 'Lacking the motivation or care to act.' },
        ],
      },
      {
        name: 'Busy',
        desc: 'Overloaded with too much to do.',
        leaves: [
          { name: 'Pressured', desc: 'Pushed by demands and expectations.' },
          { name: 'Rushed', desc: 'Hurried, with too little time.' },
        ],
      },
      {
        name: 'Stressed',
        desc: 'Strained past your capacity.',
        leaves: [
          { name: 'Overwhelmed', desc: 'Buried under more than you can manage.' },
          { name: 'Out of control', desc: "Unable to steer what's happening to you." },
        ],
      },
      {
        name: 'Tired',
        desc: 'Low on energy and reserves.',
        leaves: [
          { name: 'Sleepy', desc: 'Drowsy and needing rest.' },
          { name: 'Unfocused', desc: 'Scattered; unable to concentrate.' },
        ],
      },
    ],
  },
  {
    name: 'Surprised',
    color: 'surprised',
    desc: 'Being caught off guard by the sudden or unexpected.',
    groups: [
      {
        name: 'Startled',
        desc: 'Jolted by something sudden.',
        leaves: [
          { name: 'Shocked', desc: 'Jarred by something abrupt or upsetting.' },
          { name: 'Dismayed', desc: 'Thrown and disheartened by a bad turn.' },
        ],
      },
      {
        name: 'Confused',
        desc: 'Unable to make sense of things.',
        leaves: [
          { name: 'Disillusioned', desc: 'Let down as an illusion or belief falls away.' },
          { name: 'Perplexed', desc: 'Puzzled and unable to understand.' },
        ],
      },
      {
        name: 'Amazed',
        desc: 'Struck by something remarkable.',
        leaves: [
          { name: 'Astonished', desc: 'Greatly surprised, almost disbelieving.' },
          { name: 'In awe', desc: 'Filled with wonder and reverence.' },
        ],
      },
      {
        name: 'Excited',
        desc: 'Eager, energised anticipation.',
        leaves: [
          { name: 'Eager', desc: 'Keenly looking forward to something.' },
          { name: 'Energetic', desc: 'Full of lively energy and drive.' },
        ],
      },
    ],
  },
];

/** One outer-ring leaf, flattened with its path up to the core (for search and
 *  chip labelling). */
export interface EmotionLeaf {
  /** Qualified `Core/Leaf` identity — what a check-in stores. Unique wheel-wide. */
  token: string;
  leaf: string;
  /** Brief gloss of the leaf. */
  desc: string;
  secondary: string;
  core: string;
  color: string;
}

/** Every leaf, in wheel order, each carrying its qualified token and gloss. */
export const EMOTION_LEAVES: readonly EmotionLeaf[] = EMOTION_WHEEL.flatMap((core) =>
  core.groups.flatMap((group) =>
    group.leaves.map((leaf) => ({
      token: `${core.name}/${leaf.name}`,
      leaf: leaf.name,
      desc: leaf.desc,
      secondary: group.name,
      core: core.name,
      color: core.color,
    })),
  ),
);

/** Primary lookup: exact qualified token → entry. Unambiguous. */
const BY_TOKEN = new Map<string, EmotionLeaf>();
for (const l of EMOTION_LEAVES) BY_TOKEN.set(l.token, l);

/** Legacy fallback: bare leaf word → first wheel occurrence, preserving the
 *  pre-qualification resolution for check-ins saved before tokens existed. */
const BY_LEAF = new Map<string, EmotionLeaf>();
for (const l of EMOTION_LEAVES) if (!BY_LEAF.has(l.leaf)) BY_LEAF.set(l.leaf, l);

/** Resolve a stored word — a qualified `Core/Leaf` token or a legacy bare leaf —
 *  to its wheel entry (path + colour + gloss), or null if it isn't in the
 *  vocabulary (e.g. a word retired from a later wheel revision). */
export function emotionLeaf(word: string): EmotionLeaf | null {
  return BY_TOKEN.get(word) ?? BY_LEAF.get(word) ?? null;
}

/** Canonical stored token for a word: an already-qualified token passes through,
 *  a legacy bare leaf upgrades to its resolved `Core/Leaf`, and an unknown word
 *  is preserved verbatim (so a retired-vocabulary tag is never silently lost). */
export function emotionToken(word: string): string {
  return emotionLeaf(word)?.token ?? word;
}

/** The bare leaf word to display for a stored token (unknown words shown as-is). */
export function emotionLabel(word: string): string {
  return emotionLeaf(word)?.leaf ?? word;
}

/** The brief gloss for a stored word, or '' if it isn't in the vocabulary. */
export function emotionDesc(word: string): string {
  return emotionLeaf(word)?.desc ?? '';
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
