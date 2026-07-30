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
  - **The exception is on purpose.** `shopping_items.barcode` stays a `String`.
    It is what a phone scanned, arriving over offline sync, and a push refused
    for shape would strand an edit made with no signal. Catalog identity travels
    through `product_id`; that column is a hint, and it is typed like one.
- **Freeform has a floor.** Don't force one-offs into the catalog — that's why
  `product_id` is nullable. Promote to a product only with a barcode or when
  you'll rebuy/track it.
- **Sync (when items join it).** The offline-first layer references rows by ULID
  with nullable hard FKs resolved server-side (see `proposals/offline-first.md`),
  so cross-table links travel as ULIDs; the BIGINT FKs are for local integrity.

## Sync scope (deliberate, not an oversight)

Offline-first sync (RxDB + soft-delete + `rev`/`ulid`) currently covers **only
`shopping_items`** — the one surface you use while walking around a shop with no
signal. `items`/`locations`/`recipes` are **online-only** (plain REST, hard
deletes) **by choice**: inventory is edited at home, on wifi, so the cost of the
per-table sync machinery isn't yet worth it. When that changes, the pattern
(ULID + `rev` + tombstones + the conflict handler) is established and applies
unchanged. So the inconsistency between the two paths is a known trade-off, not
an accident.

## Deferred (not yet built)

`purchases`/`price_observations` (+ optional `shops`), recipe_ingredient →
product links (replace name-matching), items into the RxDB sync (ulid/rev),
extra item fields (opened_at/acquired_at/notes), and dropping the now-redundant
`items.barcode`/`items.name` once all reads go through the product.
