-- Life schema, migration 0026: price observations.
--
-- Price is NOT a product attribute — it varies by shop and over time — so it is
-- modelled as an append-only time series hanging off a product_listing (a given
-- shop's listing of a product). "What does Asda charge for this today" is the
-- latest observation for that listing; price history is the whole series;
-- cheapest-shop is a min over the latest-per-listing.
--
-- Money is stored in integer MINOR units (pence) — never a float — with the
-- per-unit price alongside (e.g. 892 pence per KG) for fair cross-pack
-- comparison.

CREATE TABLE IF NOT EXISTS price_observations (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    listing_id        BIGINT UNSIGNED NOT NULL,
    -- Shelf price, minor units (pence for GBP).
    amount_minor      BIGINT       NOT NULL,
    currency          VARCHAR(3)   NOT NULL DEFAULT 'GBP',
    -- Asda prices vary by nation (EN/NI/SC/WA); null when the source has one price.
    region            VARCHAR(4)   NULL,
    -- Price per unit of measure (minor units) + the measure, for fair comparison
    -- across pack sizes. e.g. 892 + 'KG' = £8.92/kg. Null when the source omits it.
    unit_amount_minor BIGINT       NULL,
    unit_measure      VARCHAR(16)  NULL,
    observed_at       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_price_listing (listing_id, observed_at),
    CONSTRAINT fk_price_listing FOREIGN KEY (listing_id)
        REFERENCES product_listings (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
