-- Life schema, migration 0040: a recipe ingredient can name a catalog product.
--
-- Ingredients have been matched to inventory by NAME since 0003 —
-- case-insensitive, exact. That is the weakest joint in the data model: "cumin",
-- "ground cumin" and "cumin seeds" are three unrelated strings, so a cupboard
-- with the right jar in it still reports the ingredient missing, and
-- "cook now" / "shopping list = recipe − stock" are wrong in the direction that
-- sends you shopping for what you already own.
--
-- `product_id` gives the line an identity to match on instead of a spelling,
-- the same shape items (0007) and shopping rows (0024) already carry — so a
-- linked ingredient and a linked item agree because they are THE SAME PRODUCT,
-- whatever either one is called.
--
-- Deliberately NULL and deliberately additive. An ingredient is a KIND of thing
-- ("cumin") while a product is one EAN ("Bart Ground Cumin 38g"), so a link can
-- only ever be an extra way to match, never a replacement: matching stays
-- product-OR-name, and an unlinked ingredient behaves exactly as it did. This
-- makes matching exact where a link exists; it does not make two brands of cumin
-- the same thing, which would need a concept ABOVE products and is not this.
--
-- ON DELETE SET NULL, as everywhere else that points at the catalog: losing a
-- product must not take the recipe line with it — the line still names a real
-- ingredient, it just goes back to matching by name.
ALTER TABLE recipe_ingredients
    ADD COLUMN IF NOT EXISTS product_id BIGINT UNSIGNED NULL AFTER name;
ALTER TABLE recipe_ingredients ADD INDEX IF NOT EXISTS idx_recipe_ingredients_product (product_id);
ALTER TABLE recipe_ingredients ADD CONSTRAINT fk_recipe_ingredients_product
    FOREIGN KEY (product_id) REFERENCES products (id) ON DELETE SET NULL;
