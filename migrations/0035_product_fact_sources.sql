-- Life schema, migration 0035: which source to trust for a whole-value fact.
--
-- Facts are stored per source (0033) and merged on read (repo::facts_for). For
-- the two whole-value facts — the nutrition panel and the ingredients text —
-- "merge" means PICK one source's account verbatim (you don't average two panels
-- or splice two ingredient lists; see nutrition::merge_nutrition). The default
-- pick is by source precedence (retailer over crowd). When the sources genuinely
-- disagree, the product page surfaces it as a divergence to approve, and your
-- pick is recorded HERE so the merge honours it and the divergence stays settled.
--
-- One row per (product, kind): `kind` is 'nutrition' or 'ingredients'; `source`
-- is the chosen source id ('asda', 'off', …). Absence = no pick yet → precedence
-- decides. Allergens and dietary are deliberately NOT here: they're safety-
-- critical and merge by union / tri-state, never by a single-source pick.

CREATE TABLE IF NOT EXISTS product_fact_sources (
    product_id BIGINT UNSIGNED NOT NULL,
    kind       VARCHAR(16)  NOT NULL,
    source     VARCHAR(16)  NOT NULL,
    decided_at DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (product_id, kind),
    CONSTRAINT fk_fact_source_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
