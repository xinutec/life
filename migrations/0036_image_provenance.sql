-- Life schema, migration 0036: the canonical picture gets provenance, so a
-- picture disagreement can be reconciled the way the scalar fields are.
--
-- Unlike name/brand/pack, the canonical image is stored as *bytes*
-- (products.image) while a listing offers only a *URL* (product_listings
-- .image_url) — so there is nothing to string-compare, and "does this shop's
-- picture differ from ours?" can't be answered from the values. What we can
-- track honestly is *which source's* picture we currently hold: a listing from a
-- DIFFERENT source that offers its own image is then a candidate to adopt (see
-- repo::picture_divergence). Adopting re-fetches that source's image through the
-- SSRF gate and records it here.
--
-- Mirrors name_source/brand_source (0025/0032). NULL = unknown origin (an image
-- that predates this column); 'user' = a hand upload, which outranks every source
-- and is never nagged for replacement.
ALTER TABLE products
    ADD COLUMN IF NOT EXISTS image_source VARCHAR(16) NULL AFTER quantity_label_source;

-- Best-effort backfill: an existing image most likely came from the row's own
-- origin ('off' for a barcoded product, the shop for an attach, 'user' for a
-- hand upload). Only where we actually hold an image.
UPDATE products SET image_source = source WHERE image IS NOT NULL AND image_source IS NULL;
