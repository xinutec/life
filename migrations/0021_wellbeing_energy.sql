-- Wellbeing fatigue → energy: unify polarity so higher = better on EVERY
-- wellbeing axis (matching mood, where 5 = great). Fatigue was stored inverse
-- (1 = none .. 5 = severe → higher = worse), which forced every consumer (the
-- trend chart, any future review/correlation) to special-case one axis. Store
-- its complement instead — energy, 1 = drained .. 5 = energetic — so nothing runs
-- backwards in the data. The UI still speaks "fatigue" (none..severe); the
-- inversion now lives only at that display boundary.
--   energy = 6 - fatigue   (none=1→5, severe=5→1).
ALTER TABLE wellbeing CHANGE COLUMN fatigue energy TINYINT UNSIGNED NULL;
UPDATE wellbeing SET energy = 6 - energy WHERE energy IS NOT NULL;
