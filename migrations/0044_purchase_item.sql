-- Life schema, migration 0044: attach a purchase to the item it bought.
--
-- 0043 identified a purchase by product id, barcode and the name it was bought
-- under. Exercising the buy flow against production for the first time showed
-- what that misses: a hand-typed buy-list row has NEITHER a product nor a
-- barcode — nothing was scanned and nothing was picked from the catalogue — so
-- `history()` (which matches on product OR barcode) can never find it. The row
-- is stored, complete and correct, and unreachable.
--
-- That is the common case, not an edge one. A buy list is mostly typed.
--
-- `item_id` is the key that always exists, because a purchase is only ever
-- recorded by buying something, and buying is what creates the item. It is the
-- fourth identifier rather than a replacement for the other three: an item can
-- be consumed and deleted while the spending record should outlive it, which is
-- why this is ON DELETE SET NULL like the product FK, never CASCADE.
--
-- Nullable for the same reason, and because every existing row predates it.

ALTER TABLE purchases
    ADD COLUMN IF NOT EXISTS item_id BIGINT UNSIGNED NULL,
    -- Named so the item's own history can ask "what did this cost" cheaply.
    ADD INDEX idx_purchases_user_item (user_id, item_id),
    ADD CONSTRAINT fk_purchases_item
        FOREIGN KEY (item_id) REFERENCES items (id) ON DELETE SET NULL;
