# Catalog vs holdings

The data model splits *what a thing is* from *what you have*. This is the
catalog-vs-inventory (SKU-vs-stock) split, and it's what makes "fetched or
manually entered, with or without a barcode" all one shape.

## The two core entities

- **Product** (`products`) — the catalog: a definition. "Yeo Valley 950 g
  Natural Yoghurt, barcode 5036589255550." **One row regardless of how many you
  own.** Some rows come from Open Food Facts (keyed by barcode), some you define
  by hand (no barcode). Single-user app → `products` is just Pippijn's catalog;
  no per-user sharing.
- **Item / holding** (`items`) — a concrete thing you possess: "a tub of that,
  in the fridge, expires 5 Jul, qty 1." **Many items → one product.**

An item links to a product via `product_id` (nullable) **or** stands alone with
its own `name`. That nullable link + name fallback is the whole trick:

| Entry | Product row | Item row |
|-------|-------------|----------|
| Barcode (OFF / typed) | looked-up / created by barcode | `product_id` → it |
| Manual, recurring | a barcode-less product you define | `product_id` → it |
| Manual, one-off | none | `product_id` NULL, `name` set |

Display name/brand/image resolve as `COALESCE(product.…, item.…)`.

## Tables and links

```
locations ──<  items        items.location_id → locations.id  (nullable, SET NULL)
locations self-tree         locations.parent_id → locations.id (CASCADE)
products  ──<  items        items.product_id   → products.id   (nullable, SET NULL)
products  ──<  shopping_items   (buy entries; same nullable-product + name pattern)
products  ──<  product_listings (one per source: that source's whole record)
items     ──<  item_history     (append-only audit)
products  (UNIQUE barcode where present)
```

Indices that matter: `products.barcode` UNIQUE (lookup + dedup); `items
(user_id)`, `items(location_id)`, `items(product_id)`, and **`items(expiry)`**
(the use-soon view).

## Deliberate modelling calls

- **Two quantities, two tables.** `products.quantity_label` = pack size
  ("950 g"); `items.quantity` = how much *you* hold. Don't conflate.
- **Batches split by expiry.** Same product, different expiry = two item rows
  (expiry is per-instance); same product + same expiry = one row with quantity.
- **Category** is canonical on the product; an item may override (freeform).
- **Image** = BLOB on the product (rides in the mysqldump backup); a `source`
  distinguishes OFF vs a photo you took (for products OFF has no image for).
- **`source` is a closed type, not a string.** Every `source` column names a
  value from `products::source::Source` (`asda | off | user | waitrose`) and
  nothing else, for the reason `nutrition::Presence` is a type: a fifth spelling
  can't be invented at a call site, `match` names every place that must change
  when a shop is added, and ts-rs gives the frontend the same union instead of a
  bare `string`. The reads *parse*, so a value in the database outside the set
  fails the query rather than arriving as a plausible default. Two consequences
  worth knowing: adding a shop is one variant plus the arms the compiler then
  demands, and the SQL that excludes non-shops is derived from `Source::is_shop`
  rather than written out.
- **The identifiers are types too.** `Barcode`, `ExternalId`, `ProductId` and
  `ListingId` live in `products::ids`. A `Barcode` is 1–14 digits and an
  `ExternalId` is `[A-Za-z0-9_-]{1,64}`, checked once at construction, which is
  what makes it safe to format either straight into an outbound URL — the reason
  is the parameter type rather than a comment asking you to trust the caller.
  `ProductId` and `ListingId` are both row numbers and deliberately *not* the
  same type: a price observation hangs off a listing, everything else off a
  product, and as `u64`s the swap type-checked.
  - **The exception is deliberate.** `shopping_items.barcode` stays a `String`.
    It is what a phone scanned, arriving over offline sync, and a push refused
    for shape would strand an edit made with no signal. Catalog identity travels
    through `product_id`; that column is a hint, and it is typed like one.
- **Freeform has a floor.** Don't force one-offs into the catalog — that's why
  `product_id` is nullable. Promote to a product only with a barcode or when
  you'll rebuy/track it.
- **Sync (when items join it).** `shopping_items` syncs; `items`/`locations`/
  `recipes` are online-only REST by choice — see [`sync.md`](sync.md), including
  the quantity-delta hazard that has to be solved before inventory joins.

## Reconciliation — the canonical row is curated, not overwritten

Each source keeps its **whole record on its own listing**, verbatim, including a
`raw_json` backstop for fields we don't model yet. The canonical `products` row
holds one **blessed** value per field with provenance, and `'user'` provenance
outranks every source — a hand correction ("Oalty" → "Oatly") survives a refresh.

A **divergence** is computed live by comparing a listing's value against the
blessed one, so there is no pending-changes table to go stale. You approve
field-by-field; the decision is recorded (`product_field_decisions`) so a
declined diff stays quiet until that source's value changes.

Two shapes fall out of this:

- **Scalars** (name, brand, pack size) string-compare directly. The image
  doesn't: the canonical picture is *bytes* and a listing offers a *URL*, so it
  is reconciled by provenance rather than by comparison (migration 0036).
- **Facts** (nutrition, allergens, ingredients, dietary flags) are stored
  per-source and merged on read. For whole-value facts — the nutrition panel,
  the ingredients text — "merge" means **pick one source's account verbatim**;
  you don't average two panels or splice two ingredient lists.

## Deferred (not yet built)

Items into the RxDB sync (see [`sync.md`](sync.md)), extra item fields
(opened_at / acquired_at / notes), and dropping the now-redundant
`items.barcode`/`items.name` once all reads go through the product.
