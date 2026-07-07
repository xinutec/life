-- Fatigue level on a wellbeing check-in: an optional second reading alongside
-- mood, so one moment can record "emotionally ok but exhausted". NULL = not
-- recorded (mood-only check-in); 1..5 = none .. severe.
ALTER TABLE wellbeing ADD COLUMN fatigue TINYINT UNSIGNED NULL AFTER score;
