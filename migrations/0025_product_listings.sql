-- Life schema, migration 0025: split the catalog into canonical products and
-- per-source listings.
--
-- Until now `products` conflated the physical product with a single source's
-- listing of it: one row carried BOTH `barcode` (the physical identity) AND
-- `source`/`external_id` (one shop's id for it), under a UNIQUE(barcode) that
-- made it impossible for two sources to describe the same barcode (which is why
-- Asda products had to be imported barcodeless — they'd collide with the Open
-- Food Facts row for the same EAN).
--
-- `product_listings` moves the per-source facet out: one canonical product
-- (keyed by its EAN in `products.barcode`) can now have many listings — Open
-- Food Facts + Asda + Waitrose + a hand-added image. `products` stays the
-- canonical row that items/shopping_items link to by id; enrichment reconciles
-- sources onto it by barcode.
--
-- Non-destructive (expand phase): `products.source`/`external_id` are left in
-- place but become vestigial (the listing is authoritative). A later migration
-- drops them once no code reads them.

CREATE TABLE IF NOT EXISTS product_listings (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    product_id   BIGINT UNSIGNED NOT NULL,
    -- Where this listing came from: 'off' | 'asda' | 'waitrose' | 'user'.
    source       VARCHAR(16)  NOT NULL,
    -- The source's own id: Open Food Facts uses the barcode; shops use their
    -- lineNumber / CIN. Unique per source.
    external_id  VARCHAR(64)  NOT NULL,
    -- Deep link to the source's product page (filled by a later increment).
    url          VARCHAR(512) NULL,
    -- The source's own title, kept verbatim (OFF's are often dirty); the clean
    -- display name lives on `products.name`.
    raw_name     VARCHAR(255) NULL,
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE KEY uniq_listing_source_external (source, external_id),
    KEY idx_listing_product (product_id),
    CONSTRAINT fk_listing_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Backfill one listing per existing catalog row. OFF rows are barcode-keyed
-- (external_id IS NULL) → the barcode becomes the listing's external_id; shop
-- rows already carry external_id. A row addressable by neither is skipped
-- (there shouldn't be any). Idempotent: re-running INSERT IGNOREs dup keys.
INSERT IGNORE INTO product_listings (product_id, source, external_id, raw_name)
SELECT id, COALESCE(source, 'off'), COALESCE(external_id, barcode), name
  FROM products
 WHERE COALESCE(external_id, barcode) IS NOT NULL;

-- Which source the canonical name currently came from — so a later increment can
-- prefer a retailer's clean name over OFF's crowd title. Backfill = the origin.
ALTER TABLE products ADD COLUMN IF NOT EXISTS name_source VARCHAR(16) NULL AFTER source;
UPDATE products SET name_source = source WHERE name_source IS NULL;
