-- Life schema, migration 0052: a removed file goes to the trash.
--
-- Removing a receipt or manual is one tap in the item's files, and a receipt is
-- what a warranty claim rests on, so it must be undoable like every other
-- delete. The bytes stay; only the listing forgets them until a restore.

ALTER TABLE item_files ADD COLUMN IF NOT EXISTS deleted_at DATETIME NULL;
