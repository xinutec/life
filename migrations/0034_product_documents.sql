-- Life schema, migration 0034: keep every fetched source document, verbatim.
--
-- Principle: anything we fetch from a source, we store — so we never fetch it
-- twice, and we keep it EXACTLY as it came. The structured facts (0027/0033) and
-- listing fields (0030) are a derived projection; if the parser improves or we
-- later want a field we don't model yet, we re-derive from this archive rather
-- than going back through a bot-wall. (Asda's product page is Cloudflare-walled —
-- refetching means driving the Android WebView again — so retaining the raw blob
-- matters especially there.)
--
-- One row per (product, source, kind) — `kind` names WHICH fetch it was ('page'
-- = Asda's Brandbank product-page blob; later 'product' for an OFF response, etc.),
-- so a source can archive more than one document about a product. Re-fetching the
-- same kind overwrites (last fetch wins) and bumps `fetched_at`. `body` is the
-- payload untouched — JSON today, but LONGTEXT so a future HTML capture fits too.
--
-- (Asda's SEARCH hit already archives verbatim on product_listings.raw_json (0030),
-- its natural per-listing home; this table is for whole-payload fetches that have
-- no listing row of their own.)

CREATE TABLE IF NOT EXISTS product_documents (
    product_id BIGINT UNSIGNED NOT NULL,
    source     VARCHAR(16)  NOT NULL,
    kind       VARCHAR(24)  NOT NULL,
    body       LONGTEXT     NOT NULL,
    fetched_at DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (product_id, source, kind),
    CONSTRAINT fk_document_product FOREIGN KEY (product_id)
        REFERENCES products (id) ON DELETE CASCADE
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
