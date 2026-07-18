-- Life schema, migration 0031: remember which source-vs-canonical disagreements
-- you've already settled.
--
-- Reconciliation (0030 stored each source's whole record) surfaces a field as a
-- divergence when a listing's value differs from the canonical `products` row.
-- Once you've decided one — adopt a source's value, or keep what you have — it
-- must not keep nagging you. But it MUST come back if the situation genuinely
-- changes: a source updates its value, or a new source arrives with a different
-- one. A decision that suppressed forever would hide real new information.
--
-- So a decision records the exact SET of values that were on the table when you
-- made it (`seen_values`, a sorted JSON array of the distinct values across the
-- canonical row and every listing). The divergence is suppressed only while that
-- set is unchanged; any change re-surfaces it. This mirrors the conflict log's
-- resolve step, but keyed by (product, field) rather than a sync row.

CREATE TABLE IF NOT EXISTS product_field_decisions (
    product_id  BIGINT UNSIGNED NOT NULL,
    -- The canonical field decided: 'name' | 'brand' | 'quantity_label'.
    field       VARCHAR(48) NOT NULL,
    -- The distinct values that were on the table at decision time, sorted, as a
    -- JSON array of strings. The suppression key: unchanged set → stay quiet.
    seen_values JSON NOT NULL,
    decided_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (product_id, field),
    CONSTRAINT fk_field_decision_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
