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
 *  Both rings are selectable: a secondary group is a legitimate answer on its
 *  own, not merely a heading over the "real" words. "Frustrated" is often the
 *  whole truth, and making you commit to Infuriated or Annoyed would record the
 *  feeling as more precise than it was.
 *
 *  Identity is the *qualified* token `Core/Name` (see [[emotionToken]]), not the
 *  bare word — because a few leaves ("Embarrassed", "Inferior", "Overwhelmed")
 *  sit under two different cores, and a bare word can't tell them apart (same
 *  name in different groups is NOT the same emotion, and their glosses differ
 *  accordingly). Within one core a name is unique across both rings, so a token
 *  always names exactly one node. The `/` delimiter is safe: no name contains a
 *  slash.
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
          // Cheeky is you being playful at someone; this is the world striking you funny.
          { name: 'Amused', desc: 'Something struck you funny.' },
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
          // The wheel had "Unfocused" and no positive twin: it could record a bad
          // day at the keyboard and not a good one.
          { name: 'Absorbed', desc: 'Lost in what you are doing; the hours and the world fall away.' },
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
          { name: 'Determined', desc: 'Set on seeing something through, however hard it gets.' },
          {
            // The rest of Powerful is agency; this is the body. The wheel could
            // record the fatigue (Bad › Tired) but not the morning it lifts.
            name: 'Energised',
            desc: 'Something in the tank today; the tiredness has lifted.',
          },
        ],
      },
      {
        // Happy's ceiling was Joyful — "bright, buoyant gladness". A day that
        // genuinely knocks you sideways charted the same as a nice Sunday.
        name: 'Elated',
        desc: 'Gladness too big to sit still with.',
        leaves: [
          { name: 'Overjoyed', desc: "Something wonderful happened and you can't stop grinning." },
          { name: 'Thrilled', desc: 'A rush of delight you can feel in your chest.' },
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
          { name: 'Calm', desc: 'Unhurried and untroubled; nothing is pulling at you.' },
          {
            // Not Calm (which claims nothing is pulling at you — something is) and
            // not Apathetic (which claims you stopped caring — you didn't).
            name: 'Accepting',
            desc: "You've stopped fighting what you can't change, and you're settled.",
          },
        ],
      },
      {
        // Warmth pointed at someone else, at either pole of their life: aching on
        // their behalf when they hurt, and glad on their behalf when they thrive.
        // The wheel had words for feeling loved and for your own grief, but none
        // for warmth carried *for another person* — so it earns a group of its own.
        name: 'Caring',
        desc: 'Warmth turned toward someone else — moved by their hurt, or glad at their good.',
        leaves: [
          {
            name: 'Compassionate',
            desc: "Moved by someone else's suffering, and wanting to ease it.",
          },
          { name: 'Tender', desc: 'Gentle and soft toward someone who is hurting.' },
          {
            // Not pride (you claim no part in it) and not admiration (esteem for
            // their quality): simply glad they exist and that it went well for them.
            name: 'Happy for them',
            desc: "Glad at someone else's good fortune or success, purely for their sake.",
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
        // Broadened past "expecting good" to include the work of getting there:
        // Hopeful has arrived, Reaching is still on the way — the commoner state on
        // a hard day, and the wheel jumped straight from Discouraged to Hopeful.
        desc: 'Turned toward a better outcome — expecting it, or working your way toward it.',
        leaves: [
          { name: 'Hopeful', desc: 'Expecting good things and looking forward.' },
          { name: 'Inspired', desc: 'Moved and uplifted to act or create.' },
          {
            name: 'Reaching',
            desc: "Working toward a better feeling you haven't reached yet — trying, on the days it doesn't come on its own.",
          },
        ],
      },
      {
        // The wheel could name the void — Sad › Empty, "hollow and without feeling
        // or meaning" — but not its opposite. It knew the absence of a point and had
        // no word for the presence of one.
        name: 'Meaningful',
        desc: 'A sense that this has a point — that what you do, or endure, matters.',
        leaves: [
          { name: 'Purposeful', desc: "You have a direction; there's something you're for." },
          { name: 'Useful', desc: 'Your being here makes a difference to something beyond you.' },
        ],
      },
      {
        // Happy could say you feel respected or valued — regard pointed *at* you —
        // but had no word for the regard you feel *toward* someone else.
        name: 'Admiring',
        desc: 'Warm regard for someone, or something, you find excellent.',
        leaves: [
          { name: 'Impressed', desc: 'Struck by how well someone did something.' },
          { name: 'Respectful', desc: 'Holding someone in high regard for who they are.' },
        ],
      },
      {
        // The wheel named the ache of what's absent (Sad › Longing) and plain
        // arousal, but not the everyday pull toward a thing you want. Its warmth is
        // provisional — craving can grip and temptation can trouble — so it sits at
        // the edge of Happy rather than its centre.
        name: 'Desiring',
        desc: 'Pulled toward something you want.',
        leaves: [
          { name: 'Craving', desc: 'A strong, bodily wanting for something in particular.' },
          { name: 'Tempted', desc: "Drawn to something you're not sure you should have." },
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
          { name: 'Longing', desc: "Aching for someone or something that isn't here." },
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
          {
            // Powerless says you can't change it. Hopeless says there is nothing
            // ahead to change — you can be perfectly capable and still see no future.
            name: 'Hopeless',
            desc: 'No way out that you can see; the future has closed.',
          },
          {
            // What's left after you had a go — unlike Powerless, which never granted
            // you any agency to begin with.
            name: 'Defeated',
            desc: 'You tried, and it beat you.',
          },
        ],
      },
      {
        // The wheel could say hope was present (Happy › Hopeful) and that it was
        // gone (Sad › Hopeless), but not that it had been knocked and needed
        // rebuilding — the commoner state by far. It gets a group of its own:
        // Despair would import a hopelessness that isn't there, and Hurt claims
        // someone wounded you, which nobody need have.
        name: 'Discouraged',
        desc: 'Your hope has taken a knock, and you have to work to get back up.',
        leaves: [
          { name: 'Disheartened', desc: 'The wind has gone out of you.' },
          {
            name: 'Deflated',
            desc: 'Something you were counting on gave way, and you sank with it.',
          },
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
          {
            // The whole Sad core could only shout — Grief, Despair, Worthless,
            // Empty. Without a mild word, an ordinary flat day had to be recorded
            // as a severe one.
            name: 'Low',
            desc: 'Quietly down, without anything being wrong.',
          },
        ],
      },
      {
        name: 'Hurt',
        desc: 'Emotionally wounded by another.',
        leaves: [
          { name: 'Embarrassed', desc: 'Self-conscious and awkward after exposure or a slip.' },
          { name: 'Disappointed', desc: "Let down when hopes weren't met." },
          {
            // Hurt held only small words; there was a chasm between Disappointed
            // and Grief with nothing in it.
            name: 'Heartbroken',
            desc: 'A loss that hurts in the body; something in you has broken.',
          },
        ],
      },
      {
        // Distinct from Lonely (no one is near) and Fearful › Rejected (someone
        // pushed you out): this is not-fitting when neither is true — the room is
        // full and welcoming and you are still out of step with it.
        name: 'Alienated',
        desc: 'Out of step with where you are — not belonging, even among people.',
        leaves: [
          { name: 'Estranged', desc: 'Grown apart from people you were once close to.' },
          { name: 'Out of place', desc: "You don't fit here, though no one has shut you out." },
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
          {
            // The rest of Disgusted is crisis-strength — Appalled, Revolted,
            // Nauseated, Horrified. Ordinary distaste had to be filed as horror.
            name: 'Put off',
            desc: "Mildly turned off; you'd rather not, and that's all it is.",
          },
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
          {
            // Annoyed is a passing bother; Resentful and Bitter are grudges against
            // a person. Neither says "this has ground on too long".
            name: 'Fed up',
            desc: "It has gone on too long. You've had enough of it.",
          },
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
          {
            // Helpless says you couldn't cope. This says you won't look — the fear
            // that stops you opening the letter or booking the appointment.
            name: 'Frozen',
            desc: "Fear has stopped you; you can't make yourself do the thing.",
          },
          {
            // Scared topped out at Frightened, so an ordinary fright and an
            // out-of-control spike had to share one word.
            name: 'Panicked',
            desc: 'Fear spiking past control; your body has hit the alarm.',
          },
          { name: 'Terrified', desc: 'Gripped by total fear; the threat feels absolute.' },
        ],
      },
      {
        name: 'Anxious',
        desc: 'Uneasy dread about what might happen.',
        leaves: [
          { name: 'Overwhelmed', desc: 'Swamped; more coming at you than you can take in.' },
          { name: 'Worried', desc: 'Anxiously turning over what could go wrong.' },
          {
            // Worry turns over what *could* go wrong. Dread has stopped asking.
            name: 'Dread',
            desc: "Certain the bad thing is coming; it isn't 'if' any more.",
          },
        ],
      },
      {
        name: 'Insecure',
        desc: 'Unsure of your footing or worth.',
        leaves: [
          { name: 'Inadequate', desc: 'Feeling not capable or good enough.' },
          { name: 'Inferior', desc: 'Feeling lesser than those around you.' },
          {
            // Inadequate and Inferior are verdicts already reached about your worth.
            // Doubt that hasn't reached one needs its own word.
            name: 'Unsure',
            desc: "Doubting yourself, without concluding you're not enough.",
          },
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
        // Arousal with no cause attached. The other Bad groups are about load
        // (Busy, Stressed) or depletion (Bored, Tired); none of them fit, and
        // filing this under Fearful › Anxious would import a fear that isn't part
        // of the feeling.
        name: 'Agitated',
        desc: 'Stirred up and unable to settle.',
        leaves: [
          { name: 'Restless', desc: "Keyed up with nowhere to put it; can't sit still." },
          { name: 'Impatient', desc: 'Chafing at the wait; wanting it to move already.' },
        ],
      },
      {
        // Being pulled two ways is not confusion (you understand it perfectly) and
        // not agitation (it has a very specific cause) — so it earns its own group
        // rather than borrowing a home that would misdescribe it.
        name: 'Conflicted',
        desc: 'Pulled two ways at once.',
        leaves: [
          { name: 'Torn', desc: 'Wanting both, and unable to choose; either way costs you.' },
          {
            name: 'Ambivalent',
            desc: 'In two minds — drawn to it and put off by it at the same time.',
          },
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
          {
            // Tired could only whisper: Sleepy and Unfocused both imply sleep would
            // fix it, so a day wiped out by treatment shared a token with a late night.
            name: 'Exhausted',
            desc: "Wrung out. Rest doesn't touch it.",
          },
        ],
      },
      {
        // Waiting had no word at all. Impatient claims a straining you may not feel,
        // Bored claims understimulation, and Fearful › Anxious imports a fear that
        // isn't part of it — the days simply do not move.
        name: 'Waiting',
        desc: 'Life on hold until you know.',
        leaves: [
          { name: 'In limbo', desc: "Nothing is decided yet, and you can't move until it is." },
          { name: 'Stuck', desc: 'Nothing you do shifts it; the days repeat.' },
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
          {
            // Surprised had no quiet register: plain "huh, didn't expect that" could
            // only be filed as Astonished or In awe.
            name: 'Taken aback',
            desc: 'Not what you expected; it stops you for a second.',
          },
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
  {
    // The other seven are all *emotion* families, so a flat, neither-up-nor-down day
    // had nowhere to go but under Sad (Empty/Low) — which files a neutral mood as a
    // low one and tints it accordingly. Neutral is its own family precisely so that
    // an "a bit nothing" day stops being recorded, and coloured, as sadness. Its hue
    // is a plain grey: the absence of a pull, not a muted version of one.
    name: 'Neutral',
    color: 'neutral',
    desc: 'Neither up nor down — level, flat, or unbothered.',
    groups: [
      {
        name: 'Flat',
        desc: 'Level and even; nothing is pulling you either way.',
        leaves: [
          { name: 'Meh', desc: 'A bit nothing; you could take it or leave it.' },
          {
            // Not Calm (a warm, settled contentment) — this claims no warmth, only
            // that nothing is swinging you. "Emotionally stable today" lands here.
            name: 'Even',
            desc: 'On an even keel — steady, and not swinging either way.',
          },
        ],
      },
    ],
  },
];

/** Anything a check-in can record: a secondary group or one of its leaves,
 *  flattened with its path up to the core (for search and chip labelling). */
export interface EmotionNode {
  /** Qualified `Core/Name` identity — what a check-in stores. Unique wheel-wide. */
  token: string;
  name: string;
  /** Brief gloss of the node. */
  desc: string;
  /** Which ring it sits in. A group is a legitimate answer in its own right —
   *  "frustrated" is often the whole truth, and forcing a leaf would make the
   *  record more precise than the feeling was. */
  kind: 'group' | 'leaf';
  /** The secondary group this node belongs to (a group node's own name). */
  secondary: string;
  core: string;
  color: string;
}

/** Every selectable node, in wheel order: each group followed by its leaves. */
export const EMOTION_NODES: readonly EmotionNode[] = EMOTION_WHEEL.flatMap((core) =>
  core.groups.flatMap((group) => [
    {
      token: `${core.name}/${group.name}`,
      name: group.name,
      desc: group.desc,
      kind: 'group' as const,
      secondary: group.name,
      core: core.name,
      color: core.color,
    },
    ...group.leaves.map((leaf) => ({
      token: `${core.name}/${leaf.name}`,
      name: leaf.name,
      desc: leaf.desc,
      kind: 'leaf' as const,
      secondary: group.name,
      core: core.name,
      color: core.color,
    })),
  ]),
);

/** Primary lookup: exact qualified token → node. Unambiguous. */
const BY_TOKEN = new Map<string, EmotionNode>();
for (const n of EMOTION_NODES) BY_TOKEN.set(n.token, n);

/** Legacy fallback: bare word → first wheel occurrence, preserving the
 *  pre-qualification resolution for check-ins saved before tokens existed.
 *  Leaves are seeded first so that where a group and a leaf share a word, an old
 *  bare value still resolves to the leaf it always displayed as. */
const BY_NAME = new Map<string, EmotionNode>();
for (const n of EMOTION_NODES) if (n.kind === 'leaf' && !BY_NAME.has(n.name)) BY_NAME.set(n.name, n);
for (const n of EMOTION_NODES) if (!BY_NAME.has(n.name)) BY_NAME.set(n.name, n);

/** Resolve a stored word — a qualified `Core/Name` token or a legacy bare word —
 *  to its wheel node (path + colour + gloss), or null if it isn't in the
 *  vocabulary (e.g. a word retired from a later wheel revision). */
export function emotionNode(word: string): EmotionNode | null {
  return BY_TOKEN.get(word) ?? BY_NAME.get(word) ?? null;
}

/** Canonical stored token for a word: an already-qualified token passes through,
 *  a legacy bare leaf upgrades to its resolved `Core/Leaf`, and an unknown word
 *  is preserved verbatim (so a retired-vocabulary tag is never silently lost). */
export function emotionToken(word: string): string {
  return emotionNode(word)?.token ?? word;
}

/** The bare word to display for a stored token (unknown words shown as-is). */
export function emotionLabel(word: string): string {
  return emotionNode(word)?.name ?? word;
}

/** The brief gloss for a stored word, or '' if it isn't in the vocabulary. */
export function emotionDesc(word: string): string {
  return emotionNode(word)?.desc ?? '';
}

/** The colour key for a stored word — the family it belongs to, or a neutral
 *  fallback for an unknown word. */
export function emotionColor(word: string): string {
  return emotionNode(word)?.color ?? 'unknown';
}

/** Case-insensitive substring search across node, secondary and core names, so
 *  typing "with" finds Withdrawn and typing "ang" surfaces the Angry family. */
export function searchEmotions(query: string): readonly EmotionNode[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return EMOTION_NODES.filter(
    (n) =>
      n.name.toLowerCase().includes(q) ||
      n.secondary.toLowerCase().includes(q) ||
      n.core.toLowerCase().includes(q),
  );
}
