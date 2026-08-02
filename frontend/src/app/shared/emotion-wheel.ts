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
 *  Every stored emotion is a token. Bare leaf names were the pre-qualification
 *  format and were resolved by a first-occurrence fallback until migration 0039
 *  rewrote the last 15 of them; that fallback is gone, and with it the hazard
 *  that adding a word ahead of an existing one silently re-pointed old history
 *  to a core it never meant. Adding to the wheel is now free of consequence for
 *  what is already recorded. */

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
          {
            // The inherited gloss ("stimulated and keenly alert") described plain
            // alertness, which is `Surprised/Energetic`, and said nothing about
            // play at all. What this word is for under Playful is the charge that
            // makes you want to play — including the frankly sexual reading, which
            // the old gloss neither named nor excluded.
            name: 'Aroused',
            desc: 'Switched on and keyed up — the charge that wants an outlet.',
          },
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
          {
            // "Bright, buoyant gladness that lifts you" contradicted its own group
            // ("quietly satisfied and at ease") and described what Elated now
            // holds — Overjoyed, Thrilled. Since Elated arrived, this is free to be
            // the quiet kind: gladness that is simply there, with no event under it.
            name: 'Joyful',
            desc: 'Gladness that is just there, without anything having happened.',
          },
          {
            // Contentment you are actively paying attention to. Not Absorbed
            // (lost in a task, the world falling away — here the world is
            // exactly what you're attending to) and not Thankful (appreciation
            // for what you have, looking back at it); this is happening now and
            // you noticed it happening.
            name: 'Savouring',
            desc: 'Taking in something small and good while it lasts, and noticing that you are.',
          },
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
            // The day yielding while you are still in it. Not Successful, which
            // is satisfaction at a finished thing, looking back at it; not
            // Absorbed, which is attention swallowed by a task and says nothing
            // about anything moving; not Determined, which is resolve against
            // friction — here there is none, which is the whole feeling.
            // Present-tense and felt, not a verdict on the day's output: the
            // word describes what it is like while the work goes, and that is
            // why it belongs to a wheel of feelings rather than to the note.
            name: 'Productive',
            desc: 'Things are getting done, and you can feel it happening.',
          },
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
          {
            // "Warm and tender affection toward others" described the Caring group,
            // not this one — and a name is unique within a core, so this cannot
            // simply be duplicated there. What Peaceful contributes is the
            // settledness: love with nothing to resolve, prove or repair.
            name: 'Loving',
            desc: 'Affection with nothing to resolve — settled, unstriving warmth.',
          },
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
          {
            // Comfort that ARRIVES, from something outside you. The rest of this
            // group is comfort by absence — Calm is nothing pulling at you,
            // Relieved is a dread that didn't land. Neither has a word for the
            // wind through the window, the warmth, the sound doing the work.
            name: 'Soothed',
            desc: 'Something outside you is easing you — a sound, a warmth, a touch doing the work.',
          },
          {
            // Where you ARE, rather than how things are going. Not Absorbed
            // (attention swallowed by a task, the world falling away — here the
            // world is precisely what you are in) and not Calm (which says
            // nothing is pulling at you, but nothing about where you are).
            // Sits under Peaceful because that is where this lands when it is
            // good; presence in pain is a different feeling and would want its
            // own word elsewhere.
            name: 'Present',
            desc: 'Here in your body and senses — not running ahead of it or looking back at it.',
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
          {
            // The rest of this group is warmth you DIRECT at someone; this is
            // warmth that arrives and lands on you. Not In awe (which is about
            // scale) and not Tender (which is aimed at someone hurting) — the
            // wheel had no word for being affected.
            name: 'Moved',
            desc: 'Something reached you — a kindness, a piece of music — and you felt it land.',
          },
        ],
      },
      {
        name: 'Trusting',
        desc: 'Feeling safe enough to open up to others.',
        leaves: [
          { name: 'Sensitive', desc: "Open and finely attuned to feeling, yours and others'." },
          { name: 'Intimate', desc: 'Emotionally close and connected to someone.' },
          {
            // The group is glossed "safe enough to open up" and had no word for
            // the safety itself — only for what safety lets you do. Nowhere in
            // the wheel could record simply feeling safe, which is about as
            // foundational as a felt state gets.
            name: 'Safe',
            desc: 'Not braced for anything — you can let your guard down here.',
          },
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
          {
            // Not Reaching, which is the EFFORT of getting better on a day it
            // won't come by itself, and not Hopeful, which expects good things
            // ahead. This is the observation that you are already further along
            // than you were — improvement you can feel, without having arrived.
            name: 'Mending',
            desc: 'Further along than you were, and not there yet.',
          },
          {
            // Twin of `Surprised/Eager`, which is where Roberts put the whole
            // Excited group — a group whose own gloss ("eager, energised
            // anticipation") has nothing to do with being caught off guard, so
            // looking forward to something could only be recorded as surprise.
            // Not Hopeful, which expects a good outcome; you can be eager for a
            // thing whose outcome you already know. What this word carries that
            // Hopeful does not is the ARRIVAL — wanting it here, not wanting it
            // to go well. Deliberately not glossed with "impatient": that is
            // `Bad/Impatient`, a chafing at the wait, and borrowing its name
            // here would point you at two places at once.
            name: 'Eager',
            desc: "Keen for something that's on its way, and wanting it here now.",
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
          {
            // Longing aches for a person, a place, a thing. This aches for a
            // TIME, and it is fond rather than painful — the bittersweet one. It
            // sits under Sad because there is a real pang in it, not only a
            // thought; `Neutral/Reflective` is where the same noticing goes when
            // it doesn't ache.
            name: 'Wistful',
            desc: 'A soft ache for a time that has passed — fond, not painful.',
          },
        ],
      },
      {
        name: 'Vulnerable',
        desc: 'Exposed and easily hurt.',
        leaves: [
          {
            // The inherited gloss described only what someone DID to you, which is
            // where Angry › Bitter › Violated lives. Under Vulnerable the feeling
            // is the exposure it leaves behind — being the one things get done to.
            name: 'Victimised',
            desc: 'Something was done to you, and it left you open to it happening again.',
          },
          { name: 'Fragile', desc: 'Easily broken or overwhelmed; delicate right now.' },
          {
            // Twin of `Happy/Sensitive`, which sits under Trusting and means being
            // OPEN — finely attuned, safe enough to feel. This is the everyday
            // reading of the word: thin-skinned today, everything landing harder
            // than it would on another day. Not Fragile, which says you might break;
            // this says the volume is up. Recording it as Happy — or as Fragile —
            // was the only option, and neither is what the word means here.
            name: 'Sensitive',
            desc: 'Thin-skinned today — everything is landing harder than it would.',
          },
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
          {
            // Twin of `Angry/Withdrawn`, which sits under a group glossed "anger
            // that withdraws" — but its own words claim no anger, and pulling
            // away because you have nothing to give is not the same feeling as
            // pulling away to shut someone out. Without this the low-mood
            // version could only be recorded as anger.
            name: 'Withdrawn',
            desc: 'Pulled back from people — not to shut them out; there is nothing to give.',
          },
          {
            // Twin of `Angry/Numb`, same reason. NOT Empty, which is hollowness —
            // this is anaesthesia: the feeling is there, your access to it is not.
            name: 'Numb',
            desc: "Deadened — you can tell there's something to feel and you can't reach it.",
          },
          {
            // Twin of `Fearful/Worthless`, whose home is Roberts' — fear of your own
            // insufficiency. But on a flat low day this is not fear, and picking the
            // only copy there recorded the day as FEAR and coloured the trend chart
            // accordingly. This is the top of the ladder the group already holds:
            // Low, Empty, Inferior — and then nothing.
            name: 'Worthless',
            desc: 'Not worth much to anyone, including yourself.',
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
          {
            // Alone in an EXPERIENCE, not alone in a room. Isolated and Lonely
            // both claim an absence of people, and picking one of them when the
            // people are right there records the wrong loneliness. Applies to
            // anything carried unshared — a diagnosis, a grief, a debt nobody
            // has been told about.
            name: 'Alone in it',
            desc: 'People are here, and not one of them is carrying what you are carrying.',
          },
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
          {
            // Every other group under Disgusted points outward; the whole core had
            // no way to turn on you. Twin of `Sad/Ashamed`, which sits under Guilty
            // and is about having fallen SHORT — a matter of standards, and it can
            // pass. This one is not about what you did: it is being repelled by
            // yourself, which is why it belongs to disgust and not to sorrow.
            name: 'Ashamed',
            desc: "Repelled by yourself — you'd rather not look at what you are.",
          },
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
          // Both of these have a twin under Sad › Depressed, and for a while only
          // the twins said what their core contributed — these two read as though
          // they were about nobody in particular, which is exactly the half of the
          // distinction a picker cannot show you. The anger is the POINT here:
          // shutting down is what this anger does instead of shouting.
          {
            name: 'Withdrawn',
            desc: 'Pulled back to shut someone out — you are not giving them you.',
          },
          { name: 'Numb', desc: 'The anger has gone cold; there is nothing where it was.' },
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
            // Something demanding is coming and you are already stiffening for
            // it. Not Dread, which needs the thing to be BAD and certain — what
            // is coming here may be neither, only costly. The positive twin,
            // `Happy/Safe`, is glossed "not braced for anything"; the wheel
            // named this state in a negation before it had a word for it.
            name: 'Braced',
            desc: 'Stiffening for something demanding that is on its way.',
          },
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
          {
            // Twin of `Disgusted/Hesitant`, which sits under Repelled ("pushed
            // away by something distasteful") — yet its own gloss says "wary",
            // which is a fear word. Holding back because you are nervous and
            // holding back because something repels you are different feelings.
            // Not Nervous, which is apprehension about what might happen; this
            // is the not-acting itself.
            name: 'Hesitant',
            desc: 'Holding back from acting, because it might go badly.',
          },
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
        // Roberts' gloss here ("eager, energised anticipation") is the same
        // sentence you would write under Happy — it describes the charge and
        // never says where it came from, which is the one thing this core is
        // for. Under Surprised the charge is not one you had built up.
        desc: 'Charge from something that has only just come into view.',
        leaves: [
          {
            // Twin of `Happy/Eager`, split on WHEN the wanting started. There
            // the thing is already on its way and you are waiting badly for it;
            // here you were not waiting at all until a moment ago. Same keenness,
            // opposite histories — and it is the history that makes this one
            // belong under Surprised.
            name: 'Eager',
            desc: "Keen for something you weren't waiting for until it appeared.",
          },
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
      {
        // Turned inward without being pulled up or down. The wheel could record
        // thinking that HURTS (Sad) or that FRIGHTENS (Fearful), and level
        // not-thinking (Flat) — but not the ordinary act of turning something
        // over, which is neither. Noticing you have changed is not the same as
        // minding, and a wheel with no word for the noticing forces the minding.
        name: 'Reflective',
        desc: 'Turned inward — thinking something over, without it pulling you up or down.',
        leaves: [
          {
            name: 'Pensive',
            desc: 'Quietly turning something over; it has some weight, but it is not troubling you.',
          },
          {
            // Deliberately NOT `Fearful/Unsure`, which sits under Insecure and
            // means doubting YOURSELF. This is doubt about what is true or what
            // you are noticing — "I feel different, but I'm not sure" — and
            // filing that under fear would record a worry nobody had.
            name: 'Uncertain',
            desc: "Not sure what you're noticing or what it means — an open question, not self-doubt.",
          },
        ],
      },
      {
        // Standing outside something could be said two ways and no others:
        // angrily (`Angry/Distant`, which shuts a person out) or sadly
        // (`Sad/Alienated`, `Sad/Numb`, which hurt). Both name the same posture
        // and then charge for a mood to go with it, so being alongside things
        // without minding got recorded as one of the two moods that happen to
        // stand the same way. This group is the posture without the mood.
        name: 'Apart',
        desc: 'Alongside it rather than in it — present, and not part of what is going on.',
        leaves: [
          {
            // Not `Angry/Withdrawn` (pulled back AT someone) and not `Sad/Numb`
            // (feeling gone where there was some). Here nothing is reaching you
            // and nothing has to. When the distance IS distressing it is one of
            // those two — or this one combined with them; grey on its own would
            // otherwise file a bad stretch as a level one.
            name: 'Detached',
            desc: 'Watching it from outside; nothing is reaching you, and nothing has to.',
          },
          {
            // The neutral twin of `Sad/Isolated` ("cut off from others, with no
            // one near"). Same fact — nobody else here — and the opposite claim
            // about it: this is the arrangement, not the deprivation.
            name: 'Solitary',
            desc: 'On your own, and that is the arrangement you want.',
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

/** Resolve a stored `Core/Name` token to its wheel node (path + colour + gloss),
 *  or null if it isn't in the vocabulary (e.g. a word retired from a later wheel
 *  revision). */
export function emotionNode(word: string): EmotionNode | null {
  return BY_TOKEN.get(word) ?? null;
}

/** Canonical stored token for a word: a known token passes through, and anything
 *  else is preserved verbatim — so a tag from a retired vocabulary is never
 *  silently lost, even though nothing can resolve it. */
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
