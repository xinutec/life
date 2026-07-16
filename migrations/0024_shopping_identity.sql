-- Life schema, migration 0024: shopping rows carry item identity.
--
-- Buying a shopping row converts it into an inventory item, but until now the
-- row carried only name/quantity/unit/barcode — the created item's category
-- was guessed (barcode → food, else other) and the row could never link to a
-- catalog product (a barcodeless shop product wasn't representable on the Buy
-- list at all). Give the row the two identity fields the conversion needs:
--
--   category   — the inventory category the buy will use. Existing rows (and
--                the column default) are 'food': the Buy list is a grocery
--                list first, and the sheet lets the user say otherwise.
--   product_id — optional link to the products catalog, same shape as
--                items.product_id (0007).
ALTER TABLE shopping_items
    ADD COLUMN IF NOT EXISTS category   VARCHAR(20)     NOT NULL DEFAULT 'food' AFTER barcode,
    ADD COLUMN IF NOT EXISTS product_id BIGINT UNSIGNED NULL AFTER category;
ALTER TABLE shopping_items ADD INDEX IF NOT EXISTS idx_shopping_product (product_id);
ALTER TABLE shopping_items ADD CONSTRAINT fk_shopping_product
    FOREIGN KEY (product_id) REFERENCES products (id) ON DELETE SET NULL;
