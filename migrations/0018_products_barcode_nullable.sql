-- Life schema, migration 0018: make products.barcode genuinely nullable.
--
-- Migration 0007 intended barcode to be optional (shop products have no EAN), and
-- ran `MODIFY COLUMN barcode ... NULL` — but at that point barcode was still the
-- PRIMARY KEY, and a PK column is implicitly NOT NULL, so the change silently did
-- not take. barcode stayed NOT NULL with no default. OFF rows always supply a
-- barcode so never noticed; the generic external import (migration 0017) omits it
-- and hits ERROR 1364 ("Field 'barcode' doesn't have a default value") under
-- MySQL strict mode. Now that `id` is the PK, this MODIFY takes effect.
ALTER TABLE products MODIFY COLUMN barcode VARCHAR(32) NULL;
