-- Life schema, migration 0039: qualify the last legacy bare emotion words, so
-- every stored feeling is a `Core/Name` token and the compatibility layer that
-- resolved bare words can be deleted.
--
-- Emotions were originally stored as bare leaf names ("Withdrawn"). Since
-- qualification they are stored as tokens ("Angry/Withdrawn"), because a bare
-- word cannot say WHICH core it meant — several names live under two. Old rows
-- kept working through a fallback that resolved a bare word to its first
-- occurrence in wheel order.
--
-- That fallback is a standing hazard, and it fired for real: adding `Withdrawn`
-- and `Numb` to Sad put a new occurrence EARLIER in wheel order than the Angry
-- ones a check-in was saved against, which would have silently re-coloured that
-- history to a core it never meant. Nothing warns you — the only reason it was
-- caught is that someone had written the assertion. Rewriting the last 15 values
-- removes the hazard rather than defending against it each time.
--
-- The target tokens are not hand-derived: they are asserted against the real
-- resolver in emotion-wheel.spec.ts ("resolves every bare word still stored in
-- production"), which lists this same set. Two of them were genuinely ambiguous
-- and resolve as they always displayed: `Overwhelmed` → Fearful (not Bad), and
-- `Disappointed` → the Sad LEAF (not the Disgusted group), because the fallback
-- seeded leaves ahead of groups.
--
-- Matching on the QUOTED word is what makes a plain REPLACE safe here: the column
-- is a JSON array of strings, so `"Hopeful"` can only match a whole element —
-- `"Happy/Hopeful"` does not contain it (the character before `Hopeful` is `/`,
-- not a quote). That also makes this idempotent: a second run finds nothing.
--
-- `wellbeing` is SYNCED, so a rewrite the clients never hear about would be
-- undone the moment a device pushed its own copy back. Each touched row therefore
-- takes a fresh revision from the shared counter, exactly as a real write would.

-- The counter's current value; each rewritten row takes the next one. A missing
-- counter row would make this NULL and the UPDATEs would fail on the NOT NULL
-- `rev` column — which is the right outcome, since silently skipping the bump
-- would leave the rewrite invisible to every device.
SET @rev = (SELECT val FROM sync_rev WHERE id = 1);

UPDATE wellbeing SET emotions = REPLACE(emotions, '"Hopeful"', '"Happy/Hopeful"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Hopeful"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Worried"', '"Fearful/Worried"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Worried"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Thankful"', '"Happy/Thankful"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Thankful"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Isolated"', '"Sad/Isolated"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Isolated"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Overwhelmed"', '"Fearful/Overwhelmed"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Overwhelmed"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Disappointed"', '"Sad/Disappointed"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Disappointed"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Annoyed"', '"Angry/Annoyed"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Annoyed"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Loving"', '"Happy/Loving"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Loving"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Sleepy"', '"Bad/Sleepy"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Sleepy"%';
UPDATE wellbeing SET emotions = REPLACE(emotions, '"Inspired"', '"Happy/Inspired"'),
  rev = (@rev := @rev + 1), updated_at = NOW() WHERE emotions LIKE '%"Inspired"%';

-- Hand the counter back, past every revision this migration issued, so the next
-- ordinary write cannot reuse one.
UPDATE sync_rev SET val = @rev WHERE id = 1;
