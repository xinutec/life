-- Life schema, migration 0050: one string per product number.
--
-- `Barcode` now parses to a canonical padding (leading zeros off, then padded
-- to 8 or 13 digits, 14 kept — Open Food Facts' own rule), so rows written
-- before it must take the same form or they stop matching new ones. Prod had no
-- two products that collapse onto one code, so nothing merges; the unique keys
-- would refuse this migration if that changed.
--
-- Only all-digit values are touched: an item's barcode is a hint and may hold
-- anything scanned. `shopping_items` is synced and left alone — its rows were
-- already canonical, and a rewrite the phones never hear about would be undone
-- by their next push. Each table goes in three steps: all zeros is a shop's
-- "none" (Asda sends `0`), then zeros off, then the padding back on.

UPDATE products SET barcode = NULL WHERE barcode REGEXP '^0+$';
UPDATE products SET barcode = TRIM(LEADING '0' FROM barcode) WHERE barcode REGEXP '^0[0-9]*$';
UPDATE products SET barcode = LPAD(barcode, 8, '0') WHERE barcode REGEXP '^[0-9]{1,7}$';
UPDATE products SET barcode = LPAD(barcode, 13, '0') WHERE barcode REGEXP '^[0-9]{9,12}$';

-- Open Food Facts keys its listing by the barcode itself.
UPDATE product_listings SET external_id = TRIM(LEADING '0' FROM external_id)
    WHERE source = 'off' AND external_id REGEXP '^0[0-9]*[1-9][0-9]*$';
UPDATE product_listings SET external_id = LPAD(external_id, 8, '0')
    WHERE source = 'off' AND external_id REGEXP '^[0-9]{1,7}$';
UPDATE product_listings SET external_id = LPAD(external_id, 13, '0')
    WHERE source = 'off' AND external_id REGEXP '^[0-9]{9,12}$';

UPDATE shop_listings SET barcode = NULL WHERE barcode REGEXP '^0+$';
UPDATE shop_listings SET barcode = TRIM(LEADING '0' FROM barcode) WHERE barcode REGEXP '^0[0-9]*$';
UPDATE shop_listings SET barcode = LPAD(barcode, 8, '0') WHERE barcode REGEXP '^[0-9]{1,7}$';
UPDATE shop_listings SET barcode = LPAD(barcode, 13, '0') WHERE barcode REGEXP '^[0-9]{9,12}$';

UPDATE items SET barcode = NULL WHERE barcode REGEXP '^0+$';
UPDATE items SET barcode = TRIM(LEADING '0' FROM barcode) WHERE barcode REGEXP '^0[0-9]*$';
UPDATE items SET barcode = LPAD(barcode, 8, '0') WHERE barcode REGEXP '^[0-9]{1,7}$';
UPDATE items SET barcode = LPAD(barcode, 13, '0') WHERE barcode REGEXP '^[0-9]{9,12}$';

UPDATE purchases SET barcode = NULL WHERE barcode REGEXP '^0+$';
UPDATE purchases SET barcode = TRIM(LEADING '0' FROM barcode) WHERE barcode REGEXP '^0[0-9]*$';
UPDATE purchases SET barcode = LPAD(barcode, 8, '0') WHERE barcode REGEXP '^[0-9]{1,7}$';
UPDATE purchases SET barcode = LPAD(barcode, 13, '0') WHERE barcode REGEXP '^[0-9]{9,12}$';
