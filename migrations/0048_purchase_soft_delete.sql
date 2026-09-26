-- Life schema, migration 0048: a removed purchase goes to the trash.

-- Removing a purchase is one tap in the item's history, so it must be undoable,
-- like deleting an item or a place. It is soft rather than re-creatable because
-- a receipt can hang off it (`item_files.purchase_id`, ON DELETE SET NULL): a
-- hard delete unlinks the receipt, and re-creating the purchase cannot relink it.

ALTER TABLE purchases ADD COLUMN IF NOT EXISTS deleted_at DATETIME NULL;
