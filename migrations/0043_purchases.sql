-- Life schema, migration 0043: what was actually paid, and where.
--
-- `price_observations` (0026) already models the SHELF price — what a shop
-- charges, scraped, hanging off that shop's listing of a product. This is the
-- other half of the same idea and deliberately NOT the same table:
--
--   * `price_observations.listing_id` is `ON DELETE CASCADE`. That is right for
--     a scrape cache — drop a listing and its price series is meaningless — and
--     catastrophic for a record of money spent. Tidying up listings must never
--     be able to delete somebody's spending history.
--   * It has no `user_id`. A shelf price is a fact about a shop; a purchase is a
--     fact about a person, and only they should read it.
--   * It requires a listing to exist. You can buy a thing in a shop this app has
--     never heard of, and that purchase is still worth recording.
--
-- So: shelf prices answer "what does Asda charge for this", purchases answer
-- "what did I pay, and where". Cheapest-shop can draw on both; only one of them
-- is evidence about your own money.
--
-- IDENTIFIED TWICE, ON PURPOSE. A purchase carries `product_id` AND `barcode`
-- AND the `name` it was bought under:
--
--   * `product_id` is the useful key and the fragile one — an item can be
--     linked to the WRONG product (found 2026-08-30: an oyster sauce pointing
--     at a honey record, its barcodes agreeing all the way down). Relinking it
--     later must not orphan what was spent, so the FK is ON DELETE SET NULL and
--     never CASCADE.
--   * `barcode` survives a relink and is what re-attaches the history afterwards.
--   * `name` survives even a barcode being wrong. It is what the thing was
--     called when it was bought, and it is the last thing that still means
--     something when both keys turn out to be lies.
--
-- Money is integer MINOR units (pence), never a float, matching 0026.
--
-- Append-only. A purchase happened; it is not edited or tombstoned, and there is
-- no rev/deleted_at because this is not synced to the client.

CREATE TABLE IF NOT EXISTS purchases (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id      VARCHAR(255)    NOT NULL,
    product_id   BIGINT UNSIGNED NULL,
    barcode      VARCHAR(32)     NULL,
    -- What it was called when bought. Not a foreign key to anything, and that is
    -- the point: it is the one field no later correction can invalidate.
    name         VARCHAR(255)    NOT NULL,
    -- Free text, not an enum: "the corner shop" is a real answer, and a closed
    -- vocabulary here would turn an unlisted shop into an unrecordable purchase.
    shop         VARCHAR(64)     NOT NULL,
    amount_minor BIGINT          NOT NULL,
    currency     VARCHAR(3)      NOT NULL DEFAULT 'GBP',
    -- The pack the price was for, copied from the buy-list row rather than asked
    -- for again — without it, £3.30 cannot be compared across pack sizes.
    quantity     DOUBLE          NULL,
    unit         VARCHAR(32)     NULL,
    bought_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_purchases_user_time (user_id, bought_at),
    KEY idx_purchases_user_barcode (user_id, barcode),
    KEY idx_purchases_user_product (user_id, product_id),
    CONSTRAINT fk_purchases_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE SET NULL
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
