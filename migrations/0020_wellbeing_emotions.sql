-- Fine-grained emotions on a wellbeing check-in: a set of feelings-wheel leaf
-- words (e.g. ["Withdrawn","Anxious"]), independent of mood and fatigue. Stored
-- as a JSON array string; NULL/absent = none recorded. The controlled vocabulary
-- lives in the frontend (emotion-wheel.ts) — the DB only holds the chosen words.
ALTER TABLE wellbeing ADD COLUMN emotions TEXT NULL AFTER fatigue;
