-- Life schema, migration 0027: product facts — nutrition, ingredients,
-- allergens, dietary flags.
--
-- These are attributes of the physical product (the canonical `products` row),
-- NOT of a single shop's listing: a tub of oats has one nutrition panel whoever
-- sells it. So they hang off `products` by id, reconciled onto one product by
-- barcode like every other enrichment. Open Food Facts is the source for the
-- quantitative panel + ingredients + allergens; dietary flags come from OFF's
-- ingredient analysis / labels (and later a retailer's own lifestyle tags).
--
-- Nutrition is modelled WIDE, not EAV: the UK mandatory back-of-pack panel is a
-- fixed, small set (the "big 8"), so one column each is simplest to query and
-- display. OFF's long tail (sodium, vitamins, nutrition score, …) that doesn't
-- fit the fixed panel is kept verbatim in a JSON `extra` column — structured
-- where it's standard, free-form where it isn't.

-- One nutrition panel per product (1:1). All figures are per the stated `basis`
-- (per 100 g for solids, per 100 ml for liquids) — the UK label convention — so
-- two products are directly comparable without knowing pack size.
CREATE TABLE IF NOT EXISTS product_nutrition (
    product_id     BIGINT UNSIGNED NOT NULL PRIMARY KEY,
    -- What the figures are per: '100g' (solids) or '100ml' (liquids).
    basis          VARCHAR(8)   NOT NULL DEFAULT '100g',
    -- The manufacturer's serving description, verbatim (e.g. '40g', '1 biscuit').
    serving_size   VARCHAR(64)  NULL,
    -- The UK "big 8" mandatory panel, per `basis`. Energy carries both units the
    -- label prints (kJ and kcal). NULL = the source didn't declare that nutrient.
    energy_kj      DOUBLE NULL,
    energy_kcal    DOUBLE NULL,
    fat_g          DOUBLE NULL,
    saturates_g    DOUBLE NULL,
    carbohydrate_g DOUBLE NULL,
    sugars_g       DOUBLE NULL,
    fibre_g        DOUBLE NULL,
    protein_g      DOUBLE NULL,
    salt_g         DOUBLE NULL,
    -- Everything else the source reported per-`basis` (sodium, vitamins, nutrition
    -- score, …), verbatim: a JSON object of nutriment → number. The promoted big-8
    -- keys are removed so this is purely the tail, no duplication.
    extra          JSON NULL,
    -- Where the numbers came from ('off').
    source         VARCHAR(16)  NOT NULL,
    updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT fk_nutrition_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Allergens for a product: one row per allergen, with how it's present. 'contains'
-- is a declared ingredient; 'may_contain' is a cross-contamination trace. The tag
-- is OFF's canonical allergen id with the language prefix stripped ('en:milk' →
-- 'milk'), so it's a stable taxonomy value, not free text.
CREATE TABLE IF NOT EXISTS product_allergens (
    product_id BIGINT UNSIGNED NOT NULL,
    allergen   VARCHAR(48)  NOT NULL,
    presence   ENUM('contains', 'may_contain') NOT NULL DEFAULT 'contains',
    source     VARCHAR(16)  NOT NULL,
    PRIMARY KEY (product_id, allergen),
    CONSTRAINT fk_allergen_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Dietary flags for a product: vegan / vegetarian / palm-oil-free (from OFF's
-- ingredient analysis, which is tri-state) and label claims like gluten-free /
-- organic / kosher / halal (present = asserted). `flag` is a stable slug; `value`
-- is 'yes' | 'no' | 'maybe' so we never over-claim (OFF "maybe vegan" stays
-- maybe, not a green tick).
CREATE TABLE IF NOT EXISTS product_dietary_flags (
    product_id BIGINT UNSIGNED NOT NULL,
    flag       VARCHAR(32)  NOT NULL,
    value      ENUM('yes', 'no', 'maybe') NOT NULL DEFAULT 'yes',
    source     VARCHAR(16)  NOT NULL,
    PRIMARY KEY (product_id, flag),
    CONSTRAINT fk_dietary_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Ingredients list is a single block of text on the product (OFF's is often the
-- cleanest structured field it has). `ingredients_source` tracks its origin so a
-- later increment can prefer a retailer's over OFF's, like `name_source`.
ALTER TABLE products ADD COLUMN IF NOT EXISTS ingredients_text   TEXT        NULL;
ALTER TABLE products ADD COLUMN IF NOT EXISTS ingredients_source VARCHAR(16) NULL;
