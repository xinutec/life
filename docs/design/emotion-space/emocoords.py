# (valence, load, direction) per token.
#   V  -10 unpleasant .. +10 pleasant
#   L    0 costs nothing to hold .. 10 costing you a lot
#   D  -10 wholly inward / about a task .. +10 wholly about another person
W = {
 'Happy': ((6,2,0), {
  'Playful':    ((7,1,2),  {'Aroused':(5,2,1),'Cheeky':(7,1,4),'Amused':(7,0,1)}),
  'Content':    ((7,0,-2), {'Free':(7,0,-3),'Lucky':(7,0,-1),'Joyful':(8,0,-1),'Savouring':(7,1,-2),'Productive':(7,1,-5)}),
  'Interested': ((5,1,-4), {'Curious':(5,1,-3),'Inquisitive':(5,2,-3),'Absorbed':(6,0,-7)}),
  'Proud':      ((7,0,-3), {'Successful':(7,0,-4),'Confident':(7,0,-3)}),
  'Accepted':   ((7,0,7),  {'Respected':(7,0,7),'Valued':(7,0,7),'Supported':(8,0,8),'Understood':(8,0,8)}),
  'Powerful':   ((6,3,-3), {'Courageous':(5,6,-2),'Creative':(7,1,-5),'Determined':(5,5,-3),
                            'Steady':(3,8,-2),'Energised':(7,0,-3)}),
  'Elated':     ((9,0,0),  {'Overjoyed':(9,0,0),'Thrilled':(9,0,0)}),
  'Peaceful':   ((7,0,0),  {'Loving':(8,0,7),'Thankful':(7,0,4),'Relieved':(7,1,-1),'Calm':(6,0,-1),
                            'Relaxed':(7,0,-2),'Accepting':(5,2,-1),'Soothed':(7,0,3),'Reassured':(6,1,3),'Present':(7,0,-2)}),
  'Caring':     ((5,2,9),  {'Compassionate':(4,3,9),'Tender':(5,2,9),'Happy for them':(8,0,9),'Moved':(6,1,7)}),
  'Trusting':   ((7,0,7),  {'Sensitive':(4,2,5),'Intimate':(8,0,9),'Safe':(8,0,4)}),
  'Optimistic': ((5,3,-2), {'Hopeful':(6,2,-2),'Inspired':(7,1,-3),'Reaching':(1,8,-2),'Mending':(3,5,-2),
                            'Eager':(6,2,-2),'Excited':(7,3,-3),'Enthusiastic':(7,2,-3)}),
  'Meaningful': ((6,2,-1), {'Purposeful':(6,2,-2),'Useful':(6,1,3)}),
  'Admiring':   ((6,0,8),  {'Impressed':(6,0,8),'Respectful':(6,0,8)}),
  'Desiring':   ((2,4,-1), {'Craving':(1,6,-2),'Tempted':(2,4,-2)}),
 }),
 'Sad': ((-6,4,0), {
  'Lonely':     ((-7,4,6), {'Isolated':(-7,4,5),'Abandoned':(-8,5,8),'Longing':(-5,5,7),'Homesick':(-5,4,2),'Wistful':(-1,1,3)}),
  'Vulnerable': ((-5,5,2), {'Victimised':(-7,5,6),'Fragile':(-5,6,0),'Sensitive':(-4,5,2)}),
  'Despair':    ((-9,6,-1),{'Grief':(-9,6,4),'Powerless':(-8,5,-1),'Hopeless':(-9,5,-2),'Defeated':(-8,4,-2)}),
  'Discouraged':((-5,4,-2),{'Disheartened':(-5,4,-2),'Deflated':(-5,3,-2)}),
  'Guilty':     ((-6,5,4), {'Ashamed':(-7,5,3),'Remorseful':(-6,5,6)}),
  'Depressed':  ((-7,3,-3),{'Inferior':(-6,4,4),'Empty':(-6,2,-4),'Low':(-4,2,-3),'Withdrawn':(-5,3,2),
                            'Numb':(-5,1,-4),'Worthless':(-8,4,-2)}),
  'Hurt':       ((-6,4,6), {'Embarrassed':(-4,4,6),'Disappointed':(-5,3,4),'Heartbroken':(-9,6,8)}),
  'Alienated':  ((-6,4,5), {'Estranged':(-6,4,7),'Out of place':(-5,4,5),'Alone in it':(-7,5,6),'Misunderstood':(-7,5,8)}),
 }),
 'Disgusted': ((-6,3,3), {
  'Disapproving':((-5,3,5),{'Judgemental':(-4,3,8),'Embarrassed':(-4,4,6),'Ashamed':(-7,5,-4)}),
  'Disappointed':((-6,3,5),{'Appalled':(-7,4,6),'Revolted':(-7,3,5)}),
  'Awful':       ((-7,3,2),{'Nauseated':(-6,2,0),'Detestable':(-7,4,6)}),
  'Repelled':    ((-5,3,2),{'Horrified':(-8,5,3),'Hesitant':(-3,4,2),'Put off':(-3,1,3)}),
 }),
 'Angry': ((-6,5,6), {
  'Let down':   ((-7,5,9), {'Betrayed':(-8,6,9),'Resentful':(-6,6,8)}),
  'Humiliated': ((-7,5,8), {'Disrespected':(-6,5,8),'Ridiculed':(-7,5,8)}),
  'Bitter':     ((-6,6,7), {'Indignant':(-5,5,7),'Violated':(-8,6,8)}),
  'Mad':        ((-6,7,7), {'Furious':(-7,8,7),'Jealous':(-6,6,7)}),
  'Aggressive': ((-5,7,8), {'Provoked':(-5,7,8),'Hostile':(-6,7,8)}),
  'Frustrated': ((-5,5,3), {'Infuriated':(-7,8,4),'Annoyed':(-3,3,4),'Exasperated':(-5,5,4)}),
  'Distant':    ((-5,4,6), {'Withdrawn':(-5,5,7),'Numb':(-4,2,5)}),
  'Critical':   ((-3,3,6), {'Sceptical':(-1,2,5),'Dismissive':(-3,3,7)}),
 }),
 'Fearful': ((-6,6,1), {
  'Scared':     ((-8,7,0), {'Helpless':(-8,6,-1),'Frightened':(-8,7,0),'Frozen':(-8,8,-1),
                            'Panicked':(-9,9,-1),'Terrified':(-9,9,0)}),
  'Anxious':    ((-6,7,0), {'Overwhelmed':(-7,8,-1),'Worried':(-5,6,1),'Braced':(-4,7,0),'Dread':(-7,7,0)}),
  'Insecure':   ((-5,5,2), {'Inadequate':(-6,5,2),'Inferior':(-6,5,4),'Unsure':(-3,4,0)}),
  'Weak':       ((-7,4,2), {'Worthless':(-8,4,0),'Insignificant':(-6,3,4)}),
  'Rejected':   ((-7,5,8), {'Excluded':(-6,4,8),'Persecuted':(-8,6,8),'Losing them':(-7,7,9)}),
  'Threatened': ((-6,7,4), {'Nervous':(-4,6,2),'Exposed':(-6,6,4),'Hesitant':(-3,5,2),'Reluctant':(-4,6,1),'Guarded':(-4,9,8)}),
 }),
 'Bad': ((-4,4,-3), {
  'Bored':     ((-2,1,-4), {'Indifferent':(-1,0,-3),'Apathetic':(-3,1,-4)}),
  'Agitated':  ((-3,5,-3), {'Restless':(-3,5,-3),'Impatient':(-3,4,-2)}),
  'Conflicted':((-3,6,-2), {'Torn':(-4,7,-1),'Ambivalent':(-2,4,-2)}),
  'Busy':      ((-3,6,-4), {'Pressured':(-4,7,-2),'Rushed':(-3,6,-4)}),
  'Stressed':  ((-5,8,-2), {'Overwhelmed':(-6,8,-2),'Out of control':(-7,8,-2),'Performing':(-4,9,8),'Trapped':(-7,8,0)}),
  'Tired':     ((-3,2,-4), {'Sleepy':(-1,1,-5),'Unfocused':(-2,2,-5),'Exhausted':(-5,3,-4),'Drained':(-4,4,6),
                            'Wired':(-2,4,-5),'Foggy':(-3,2,-6),'Weary':(-5,4,-3)}),
  'Unwell':    ((-5,3,-6), {'Feverish':(-5,3,-6),'Shaky':(-4,3,-6)}),
  'Waiting':   ((-4,5,-2), {'In limbo':(-4,5,-1),'Stuck':(-5,5,-3)}),
 }),
 'Surprised': ((0,3,0), {
  'Startled': ((-2,4,0), {'Shocked':(-4,5,0),'Dismayed':(-4,4,0),'Taken aback':(-1,3,1)}),
  'Confused': ((-3,4,-1),{'Disillusioned':(-5,4,3),'Perplexed':(-1,4,-2)}),
  'Amazed':   ((6,1,2),  {'Astonished':(5,1,1),'Awestruck':(7,0,2)}),
  'Excited':  ((6,2,-1), {'Eager':(6,2,-1),'Energetic':(7,1,-2)}),
 }),
 'Neutral': ((0,1,-2), {
  'Flat':      ((0,0,-3), {'Meh':(0,0,-3),'Even':(1,1,-2)}),
  'Reflective':((1,2,-4), {'Pensive':(1,2,-3),'Uncertain':(0,3,-3),'Not myself':(-2,3,-7)}),
  'Apart':     ((0,1,0),  {'Detached':(0,1,2),'Unreal':(-2,2,0),'Solitary':(2,0,-3)}),
 }),
}
def tokens():
    out={}
    for core,(cxyz,groups) in W.items():
        out[core]=cxyz
        for g,(gxyz,leaves) in groups.items():
            out[f"{core}/{g}"]=gxyz
            for l,lxyz in leaves.items(): out[f"{core}/{l}"]=lxyz
    return out
