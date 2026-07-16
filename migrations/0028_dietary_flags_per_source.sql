-- Life schema, migration 0028: dietary flags become multi-source.
--
-- 0027 keyed dietary flags (product_id, flag) and wrote them with a
-- whole-product REPLACE, which only holds while ONE authority supplies them
-- (Open Food Facts). Asda's storefront also tags its products — Vegan,
-- Vegetarian, Halal, Kosher, NoGluten, NoLactose (its NUTRITIONAL_INFO block) —
-- and those are worth keeping: they're the retailer's own claim about the exact
-- product they sell. Under the old key, storing Asda's flags and then looking
-- the barcode up on OFF again would silently DELETE Asda's rows.
--
-- So each source now keeps its own claims, and the read side merges them
-- (see repo::facts_for): agreement stands, and a genuine yes/no disagreement
-- degrades to 'maybe' rather than picking a winner — the tri-state exists
-- precisely so we never over-claim "vegan" to someone avoiding animal products.
--
-- Existing rows all carry source 'off', so widening the key can't collide.

-- Drop and add in ONE statement, deliberately: the `fk_dietary_product` foreign
-- key is indexed by this very primary key (product_id is its leftmost column),
-- so dropping it on its own leaves the constraint unindexed and MariaDB refuses
-- with errno 150 ("Foreign key constraint is incorrectly formed"). Combined, the
-- new key takes over that duty in the same operation and the FK is never
-- without an index.
ALTER TABLE product_dietary_flags
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (product_id, source, flag);
