-- Life schema, migration 0017: source-scoped external ids for the catalog.
--
-- `products.source` already discriminates where a catalog row came from
-- ('off' = Open Food Facts, 'user' = hand-entered). Shop integrations add more
-- sources (e.g. 'waitrose'), but a shop product is NOT keyed by an EAN barcode —
-- it has the shop's own id (a Waitrose "lineNumber", an Ocado SKU, …). Add a
-- generic, source-scoped `external_id` so such a product is uniquely addressable
-- and de-duped on re-import, without baking any one shop into the schema.
--
-- The unique key is (source, external_id). MySQL treats NULLs as distinct in a
-- unique index, so existing rows (source set, external_id NULL) are unaffected
-- and any number of them coexist — the constraint only bites once a source
-- supplies a non-NULL id.
ALTER TABLE products ADD COLUMN external_id VARCHAR(64) NULL AFTER barcode;
ALTER TABLE products ADD UNIQUE KEY uniq_products_source_external (source, external_id);
