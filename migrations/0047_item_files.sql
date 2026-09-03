-- Life schema, migration 0047: receipts and manuals, attached to a thing you own.

-- The catalogue already stores one image per PRODUCT, keyed on a barcode. That
-- is the wrong shape for this twice over: products are shared reference data and
-- a receipt is personal, and an appliance entered by hand has no barcode and no
-- product to hang anything on. So files hang off the ITEM, which is the one key
-- that always exists.
--
-- `purchase_id` is nullable and carries the distinction that matters. A RECEIPT
-- is evidence of a particular transaction — the same reason `warranty_months`
-- lives on the purchase and not the item (migration 0046): buy the same model
-- twice and you have two receipts, and one attached only to the item could not
-- say which. A MANUAL belongs to the thing and has no purchase, so the column is
-- NULL. ON DELETE SET NULL rather than CASCADE: deleting a mistyped purchase
-- must not silently take a scanned receipt with it.
--
-- ⚠ NO `kind` COLUMN, deliberately. A closed vocabulary invented before any real
-- file exists is how `items.category` shipped as five values and had to be
-- widened to ten the moment a house was actually entered into it. The filename
-- and the purchase link say what a file is; when there are enough files to show
-- that they group some other way, the grouping can be added knowing what it is.
--
-- Bytes live in the row, like `products.image`. Not because a blob store would
-- be wrong, but because there is exactly one of everything here and an object
-- store is a second thing to back up, secure and restore for no gain at this
-- size. `size_bytes` is stored so a listing can be answered without reading a
-- single blob — the whole reason the list and the download are separate routes.

CREATE TABLE IF NOT EXISTS item_files (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id     VARCHAR(255)    NOT NULL,
    item_id     BIGINT UNSIGNED NOT NULL,
    purchase_id BIGINT UNSIGNED NULL,
    -- What the person called it, or what their phone called it. Free text: a
    -- filename is a label, not an identifier, and nothing keys on it.
    name        VARCHAR(255)    NOT NULL,
    -- The SNIFFED type, never the declared one. See the handler.
    mime        VARCHAR(64)     NOT NULL,
    size_bytes  BIGINT UNSIGNED NOT NULL,
    bytes       LONGBLOB        NOT NULL,
    created_at  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_item_files_user_item (user_id, item_id),
    KEY idx_item_files_purchase (purchase_id),
    CONSTRAINT fk_item_files_item FOREIGN KEY (item_id)
        REFERENCES items (id) ON DELETE CASCADE,
    CONSTRAINT fk_item_files_purchase FOREIGN KEY (purchase_id)
        REFERENCES purchases (id) ON DELETE SET NULL
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
