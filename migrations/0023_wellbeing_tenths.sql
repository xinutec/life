-- Half-steps on the 1..5 wellbeing scale: "4, but a bit lower during the gym".
--
-- The readings are now stored in TENTHS: 10..50, where 35 is a 3.5. Fixed-point
-- integers, not a real number — they average and compare exactly (a mean of 3.5
-- and 4 is 37.5 tenths, not 3.4999999999999996), and TINYINT UNSIGNED already
-- holds the range, so this is a rescale rather than a type change.
--
-- The columns are RENAMED, not just rescaled. A scaled integer's failure mode is
-- a 4 read as a 0.4 — by a stale client, a half-applied migration, or a hand-
-- written query months from now. Renaming makes that impossible: code that still
-- says `score` fails to find the column instead of quietly plotting a good day as
-- an awful one. The name also states the scale to anyone reading the table.
--
-- The domain rule (steps of 5 = half-points) lives in the backend's validation,
-- not here: the column can hold 37, the app refuses it. Allowing finer steps
-- later is a change to that rule alone, with no migration.
ALTER TABLE wellbeing
    CHANGE COLUMN score  score_tenths  TINYINT UNSIGNED NOT NULL,
    CHANGE COLUMN energy energy_tenths TINYINT UNSIGNED NULL;

UPDATE wellbeing
   SET score_tenths  = score_tenths * 10,
       energy_tenths = energy_tenths * 10;
