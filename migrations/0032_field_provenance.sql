-- Life schema, migration 0032: brand and pack size get the same provenance the
-- name has, so "our own" corrections to them survive a source refresh.
--
-- `name_source` (0025) marks where the canonical name came from, and `'user'`
-- there means a hand correction that outranks every source and is never
-- auto-overwritten (the "Oalty" → "Oatly" case). Brand and pack size had no
-- such column, so the reconcile UI could adopt a source's value for them but
-- could not hold a typed correction against a later re-import — and the very
-- first thing noticed on the product page was a pack-size spelling ("250ML" vs
-- "250ml") that only a user layer can fix, since it's the shop's own casing.
--
-- So mirror `name_source` for the other two reconcilable scalar fields. NULL
-- means "no explicit provenance yet" (a plain fill from a source); `'user'`
-- means our own, protected value. Additive and nullable; existing rows keep
-- NULL until reconciled.

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS brand_source          VARCHAR(16) NULL AFTER name_source,
    ADD COLUMN IF NOT EXISTS quantity_label_source VARCHAR(16) NULL AFTER brand_source;
