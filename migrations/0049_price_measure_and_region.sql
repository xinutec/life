-- Life schema, migration 0049: one spelling per price measure; no region.

-- A per-unit price's measure is a closed set, `UnitMeasure` ("KG", "L",
-- "each"), matching what purchases derive. Asda's own spellings were stored
-- verbatim ("LT", "EA"), so the same measure printed two ways; they are
-- rewritten here, and anything else is dropped rather than guessed at.
UPDATE price_observations SET unit_measure = 'L' WHERE unit_measure = 'LT';
UPDATE price_observations SET unit_measure = 'each' WHERE unit_measure = 'EA';
UPDATE price_observations SET unit_measure = NULL
    WHERE unit_measure IS NOT NULL AND unit_measure NOT IN ('KG', 'L', 'each');

-- `region` only ever held Asda's "EN" (the parser always picks England) and
-- nothing read it.
ALTER TABLE price_observations DROP COLUMN IF EXISTS region;
