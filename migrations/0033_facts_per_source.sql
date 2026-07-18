-- Life schema, migration 0033: nutrition, allergens and ingredients become
-- multi-source, the way dietary flags already are (0028).
--
-- 0027 stored one nutrition panel, one allergen set and one ingredients block per
-- product, keyed by product_id alone: fine while Open Food Facts was the sole
-- authority. But Asda's product page carries its own Brandbank facts (a full
-- panel, ingredients, and allergen advice), and those are worth keeping beside
-- OFF's — a retailer's manufacturer-grade data next to the crowd's. Under the old
-- keys, storing Asda's facts and then re-looking-up the barcode on OFF would
-- overwrite them (nutrition/ingredients) or delete them (allergens' whole-product
-- replace). So each source keeps its own, and the read side merges them
-- (repo::facts_for): nutrition/ingredients pick by source precedence, allergens
-- UNION with the most-severe presence winning — an allergen one source declares
-- is never dropped because another is silent about it.
--
-- Existing rows all carry source 'off', so widening the keys can't collide.

-- product_nutrition: 1-per-product → 1-per-(product, source).
-- Drop+add the primary key in ONE statement: fk_nutrition_product is indexed by
-- the current PK (product_id leftmost), so a bare DROP would leave the FK
-- unindexed (errno 150). The new PK keeps product_id leftmost, so it takes over
-- that duty in the same operation (same reasoning as 0028).
ALTER TABLE product_nutrition
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (product_id, source);

-- product_allergens: 1-per-(product, allergen) → 1-per-(product, source, allergen).
ALTER TABLE product_allergens
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (product_id, source, allergen);

-- Ingredients move off the two `products` columns (0027) onto their own per-source
-- table, mirroring the other facts. One block of text per source.
CREATE TABLE IF NOT EXISTS product_ingredients (
    product_id BIGINT UNSIGNED NOT NULL,
    source     VARCHAR(16)  NOT NULL,
    text       TEXT NOT NULL,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (product_id, source),
    CONSTRAINT fk_ingredients_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Carry existing ingredients across (source defaults to 'off' for any legacy row
-- that recorded text but no source).
INSERT INTO product_ingredients (product_id, source, text)
SELECT id, COALESCE(ingredients_source, 'off'), ingredients_text
FROM products
WHERE ingredients_text IS NOT NULL AND ingredients_text <> '';

-- The old columns are now a second, stale source of truth — cut them.
ALTER TABLE products DROP COLUMN ingredients_text;
ALTER TABLE products DROP COLUMN ingredients_source;
