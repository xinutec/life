-- Optional priority (high/medium/low) on a to-do, so the list can triage.
-- NULL = unprioritised (sorts last).
ALTER TABLE todos ADD COLUMN IF NOT EXISTS priority VARCHAR(8) NULL;
