-- Life schema, migration 0029: remember what the shops told us.
--
-- Every shop query we make returns more than the one product we asked about: an
-- Asda search hands back ~15 hits, each carrying its own EAN in `IMAGE_ID`. Up
-- to now we read the one hit that matched and dropped the rest — so the next
-- "Find at Asda" for a different product paid for a search we had already run.
--
-- `shop_listings` is our memory of the shops' catalogues, NOT our catalogue.
-- It is deliberately NOT `product_listings`: that table's `product_id` is NOT
-- NULL, so storing a hit there would mean minting a canonical `products` row
-- (and its inline image blob) for every incidental search result, filling the
-- catalogue that items and shopping rows link to with things nobody asked for.
-- Rows here are free-floating and keyed by the shop's own identity; a row is
-- promoted into a real `product_listings` row only when it's matched to a
-- product and attached.
--
-- This is a cache of OUR OWN queries — only ever what a search we ran returned.
-- Nothing here enumerates or crawls a shop's catalogue.
CREATE TABLE IF NOT EXISTS shop_listings (
    -- 'asda' | 'waitrose'. No 'off'/'user': this is shop-query memory.
    source         VARCHAR(16)  NOT NULL,
    -- The shop's own id: Asda's CIN, Waitrose's lineNumber.
    external_id    VARCHAR(64)  NOT NULL,
    -- The EAN, when the shop gave us one. NULL is meaningful: Waitrose search
    -- hits carry no barcode (it arrives only with a full product fetch), so a
    -- NULL row records "seen, barcode still unknown" rather than "no barcode".
    barcode        VARCHAR(32)  NULL,
    name           VARCHAR(255) NULL,
    brand          VARCHAR(255) NULL,
    quantity_label VARCHAR(64)  NULL,
    -- A URL, never bytes: unlike `products.image` (a longblob for the catalogue
    -- rows we care about), a cache of incidental hits must not carry blobs.
    image_url      VARCHAR(512) NULL,
    first_seen_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (source, external_id),
    -- The lookup this table exists to answer: "does <shop> carry <barcode>?".
    KEY idx_shop_listing_barcode (source, barcode)
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
