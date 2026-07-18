-- Life schema, migration 0030: the per-source listing keeps the source's whole
-- record, not just its name.
--
-- `product_listings` (0025) was the per-source facet of a product, but it only
-- ever stored `url` + `raw_name`: everything else a source told us (its brand,
-- its pack size, its image, the rest of the payload) was either flattened onto
-- the canonical `products` row — silently overwriting or silently ignored — or
-- dropped at parse time. That made the source's own account of the product
-- unrecoverable, and left the canonical fields with no honest record of where
-- they came from or what else was on offer.
--
-- So the listing now holds the source's account verbatim: its own brand, pack
-- size and image URL as structured columns, plus `raw_json` — the source's
-- ENTIRE record, untouched — so a field we don't model yet is kept rather than
-- lost. The canonical `products` row becomes a curated choice among these
-- listings (a later increment surfaces the differences to approve), and every
-- blessed value can be traced back to the line it came from.
--
-- Additive and nullable: existing listings (backfilled from the flat table in
-- 0025) simply carry NULLs here until their source is next pulled.

ALTER TABLE product_listings
    -- The source's own brand + pack size, as it gave them (canonical
    -- `products.brand`/`quantity_label` may differ once reconciled).
    ADD COLUMN IF NOT EXISTS brand          VARCHAR(255) NULL AFTER raw_name,
    ADD COLUMN IF NOT EXISTS quantity_label VARCHAR(64)  NULL AFTER brand,
    -- The source's image on its own CDN (a URL, never bytes — bytes live on
    -- `products.image` once a source's picture is adopted).
    ADD COLUMN IF NOT EXISTS image_url      VARCHAR(512) NULL AFTER quantity_label,
    -- The source's complete record, verbatim. JSON so it stays queryable, but
    -- read lazily: the listing getters do NOT select it (it can be large).
    ADD COLUMN IF NOT EXISTS raw_json       JSON         NULL AFTER image_url;
