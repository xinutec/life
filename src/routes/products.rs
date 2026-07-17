//! Product lookup: cache-first, Open Food Facts on a miss; plus image serving.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use crate::error::AppError;
use crate::products::prices::PriceInput;
use crate::products::types::{Product, ProductDetail, ProductListing};
use crate::products::{asda, off, repo, shop_cache, source};
use crate::session::AuthUser;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: String,
}

/// GET /api/products/shop/asda?q= → live name search against Asda's storefront
/// (see products::asda). Distinct from the local catalog tier at
/// GET /api/products: this hits the shop, so the picker offers it as its own
/// explicit tier. A blank query returns `[]` with no outbound call.
///
/// Every hit is remembered on the way past (products::shop_cache), not just the
/// one the caller ends up using: each carries its own EAN, so a search we've
/// already paid for teaches us ~15 barcode → CIN mappings. Dropping them meant
/// re-querying Asda for a product this very search had already described.
pub async fn search_asda(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<asda::AsdaHit>>, AppError> {
    let hits = asda::search(&app.http, params.q.trim(), 15).await?;
    remember_hits(&app.pool, &hits).await;
    Ok(Json(hits))
}

/// Cache what a search showed us. Deliberately infallible from the caller's
/// side: remembering is a side benefit of a query the user asked for, so a
/// cache write that fails must not turn their working search into an error.
/// It's logged, not swallowed silently — a cache that quietly never writes
/// would look exactly like one that's working.
async fn remember_hits(pool: &sqlx::MySqlPool, hits: &[asda::AsdaHit]) {
    let listings: Vec<shop_cache::CachedListing> = hits
        .iter()
        .map(shop_cache::CachedListing::from_asda)
        .collect();
    if let Err(e) = shop_cache::remember(pool, &listings).await {
        tracing::warn!(
            "shop_cache: failed to remember {} asda hits: {e:#}",
            listings.len()
        );
    }
}

/// GET /api/products?q= → catalog name/brand substring search (the product
/// picker's catalog tier). Catalog-only and cheap: no OFF/shop traffic — the
/// external tiers are separate, explicit actions in the picker.
pub async fn search(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<Product>>, AppError> {
    let q = params.q.trim();
    if q.is_empty() {
        return Ok(Json(vec![]));
    }
    Ok(Json(repo::search(&app.pool, q, 20).await?))
}

/// GET /api/products/{barcode} → cached metadata, fetching+caching from OFF on
/// a miss. 404 if OFF has no such product.
pub async fn lookup(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<String>,
) -> Result<Json<Product>, AppError> {
    if let Some(p) = repo::get(&app.pool, &barcode).await? {
        tracing::debug!(%barcode, "product cache hit");
        return Ok(Json(p));
    }
    let Some(found) = off::fetch(&app.http, &barcode).await? else {
        tracing::debug!(%barcode, "product not in Open Food Facts");
        return Err(AppError::NotFound);
    };
    tracing::debug!(%barcode, name = ?found.name, has_image = found.image_url.is_some(), "product fetched from Open Food Facts");
    // Image fetch failure is non-fatal (the product still caches) but must be
    // visible — guard rejections inside fetch_image already log; so does this.
    let image = match &found.image_url {
        Some(url) => off::fetch_image(url).await.unwrap_or_else(|e| {
            tracing::warn!(%barcode, %url, error = %e, "product image fetch failed");
            None
        }),
        None => None,
    };
    repo::upsert(
        &app.pool,
        &barcode,
        found.name.as_deref(),
        found.brand.as_deref(),
        found.quantity.as_deref(),
        image,
    )
    .await?;
    let product = repo::get(&app.pool, &barcode)
        .await?
        .ok_or(AppError::NotFound)?;
    // Record the 'off' listing so this product joins the source model (its
    // barcode is Open Food Facts' own id for it).
    repo::upsert_listing(
        &app.pool,
        product.id,
        "off",
        &barcode,
        None,
        found.name.as_deref(),
    )
    .await?;
    // Store the nutrition/ingredients/allergens/dietary facts from the same OFF
    // response, attached to the canonical product.
    repo::store_facts(&app.pool, product.id, &found.facts, "off").await?;
    // The upsert restated OFF's crowd name; give the preferred source (a
    // retailer listing, if the product has one) the last word, and answer with
    // the settled row.
    repo::refresh_canonical_name(&app.pool, product.id).await?;
    repo::get_by_id(&app.pool, product.id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// PUT /api/products/{barcode}/image → replace the cached image with the raw
/// bytes in the request body (Content-Type names the mime). The frontend sends
/// the picked/pasted/dropped blob straight through, so there's no multipart to
/// parse. Body size is bounded by a per-route `DefaultBodyLimit` (see the router)
/// and re-checked here. Returns 204 on success.
pub async fn set_image(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    if !off::is_valid_barcode(&barcode) {
        return Err(AppError::BadRequest(
            "barcode must be up to 14 digits".into(),
        ));
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Friendly rejection for obviously-wrong uploads; the declared mime is
    // otherwise only advisory — the bytes decide (below).
    if off::accept_upload_mime(content_type).is_none() {
        return Err(AppError::BadRequest(
            "Content-Type must be a raster image type (jpeg/png/gif/webp/avif)".into(),
        ));
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("empty image".into()));
    }
    if body.len() > off::MAX_UPLOAD_BYTES {
        return Err(AppError::BadRequest("image exceeds 5 MiB".into()));
    }
    // Store what the bytes actually are, not what the header claims.
    let Some(mime) = off::sniff_image_mime(&body) else {
        return Err(AppError::BadRequest(
            "the uploaded bytes are not a recognized image".into(),
        ));
    };
    repo::set_image(&app.pool, &barcode, &body, mime).await?;
    tracing::info!(%barcode, bytes = body.len(), %mime, "product image replaced");
    Ok(StatusCode::NO_CONTENT)
}

/// A product to fold into the catalog from an external source. The client (which
/// has the source's data — e.g. a Waitrose product looked up by lineNumber) sends
/// already-normalized fields; the backend stays source-agnostic.
#[derive(serde::Deserialize)]
pub struct ImportProduct {
    /// Registered source id ('waitrose', …). See products::source.
    pub source: String,
    /// Source-scoped id (e.g. a Waitrose lineNumber).
    pub external_id: String,
    pub name: String,
    pub brand: Option<String>,
    /// The product's EAN, when the source knows it (Asda's IMAGE_ID, a Waitrose
    /// barCode). Reconciles this listing onto the canonical product for that
    /// barcode, so shop + Open Food Facts data merge into one product.
    pub barcode: Option<String>,
    /// Optional image on the source's CDN; fetched server-side, host-allowlisted.
    pub image_url: Option<String>,
    /// Optional price the source quoted; appended to the listing's price history.
    pub price: Option<PriceInput>,
    // NB: no `category` — `products.category` is the short ItemCategory enum
    // (food/medication/…), not a shop taxonomy. Mapping a shop's categories is a
    // separate increment.
}

/// POST /api/products/import → upsert a catalog row from an external source,
/// keyed on (source, external_id). Idempotent: re-importing refreshes the row.
/// An `image_url` that passes the source's host allowlist is fetched from the
/// source CDN and stored (served back via /api/products/id/{id}/image).
pub async fn import(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(body): Json<ImportProduct>,
) -> Result<Json<Product>, AppError> {
    let Some(src) = source::importable(&body.source) else {
        return Err(AppError::BadRequest(format!(
            "unknown import source: {}",
            body.source
        )));
    };
    let ext = body.external_id.trim();
    if ext.is_empty()
        || ext.len() > 64
        || !ext
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(AppError::BadRequest(
            "external_id must be 1-64 chars of [A-Za-z0-9_-]".into(),
        ));
    }
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let brand = body
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    // A supplied barcode must be a real EAN before we key a canonical product on
    // it (same guard as the OFF lookup path).
    let barcode = body
        .barcode
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if let Some(bc) = barcode
        && !off::is_valid_barcode(bc)
    {
        return Err(AppError::BadRequest(
            "barcode must be up to 14 digits".into(),
        ));
    }
    let product = repo::upsert_external(
        &app.pool,
        &body.source,
        ext,
        barcode,
        Some(name),
        brand,
        None,
    )
    .await?;
    tracing::info!(source = %body.source, external_id = %ext, ?barcode, name, "product imported");

    // Optional price: append an observation to this listing's history. Best-effort
    // relative to the import — a missing listing id (shouldn't happen) just skips it.
    if let Some(price) = &body.price
        && let Some(lid) = repo::listing_id(&app.pool, &body.source, ext).await?
    {
        repo::record_price(&app.pool, lid, price).await?;
        tracing::info!(source = %body.source, external_id = %ext, amount_minor = price.amount_minor, "price recorded");
    }

    // Optional image: SSRF-gated against the source's host allowlist, fetched
    // from the source CDN. A failed fetch just leaves the row image-less.
    if let Some(url) = body.image_url.as_deref().filter(|s| !s.is_empty())
        && !src.image_hosts.is_empty()
        && let Some((bytes, mime)) = off::fetch_image_from(url, src.image_hosts).await?
    {
        repo::set_image_by_id(&app.pool, product.id, &bytes, &mime).await?;
        return repo::get_by_id(&app.pool, product.id)
            .await?
            .map(Json)
            .ok_or(AppError::NotFound);
    }
    Ok(Json(product))
}

/// GET /api/products/id/{id} → everything the product page shows in one fetch:
/// the canonical product, its per-source listings (deep links resolved), the
/// latest price per shop (cheapest first), and its facts. Prices and facts are
/// empty until a shop quote / OFF lookup has provided them.
pub async fn product_detail(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Json<ProductDetail>, AppError> {
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (listings, prices, facts) = tokio::try_join!(
        repo::listings_for(&app.pool, id),
        repo::latest_prices(&app.pool, id),
        repo::facts_for(&app.pool, id),
    )?;
    let listings = listings
        .into_iter()
        .map(|l| ProductListing {
            url: l
                .url
                .clone()
                .or_else(|| source::listing_url(&l.source, &l.external_id)),
            source: l.source,
            external_id: l.external_id,
            raw_name: l.raw_name,
        })
        .collect();
    Ok(Json(ProductDetail {
        product,
        listings,
        prices,
        facts,
    }))
}

/// The answer to "does this shop carry this product?".
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShopFind {
    /// The barcode-confirmed listing, or `None` for "we looked and this shop
    /// doesn't carry this barcode" — never "we gave up early".
    pub hit: Option<asda::AsdaHit>,
    /// Whether this came from memory rather than a fresh shop query. The UI says
    /// so: an answer we already had and one we just paid for are different
    /// things, and hiding which is which makes the cache unfalsifiable.
    pub from_cache: bool,
}

/// A remembered listing, shaped as a search hit.
///
/// Price and dietary flags are deliberately absent rather than stale: the cache
/// keeps identity (this barcode is this CIN), which doesn't rot, and not the
/// figures that do. Attaching re-fetches those from the shop for real, so the
/// only thing this has to be good enough for is letting you confirm it's the
/// right product.
fn cached_as_hit(c: shop_cache::CachedListing) -> asda::AsdaHit {
    asda::AsdaHit {
        external_id: c.external_id,
        name: c.name.unwrap_or_default(),
        brand: c.brand,
        barcode: c.barcode,
        quantity_label: c.quantity_label,
        price_label: None,
        price: None,
        image_url: c.image_url,
        dietary: vec![],
    }
}

/// GET /api/products/id/{id}/find/{source} → does this shop carry this product's
/// barcode?
///
/// Memory first, shop second. A hit in `shop_listings` answers with no outbound
/// traffic at all; only a miss asks the shop, and that query's whole result is
/// remembered on the way back, so the cache fills as a side effect of use and
/// lookups tend toward zero queries.
///
/// Identity is always the barcode, never the name: Asda's relevance order is no
/// evidence about which product this is (a name search for a balsamic ranked a
/// raspberry glaze above it). So a `None` here means every hit was checked and
/// none carried this EAN — a real, if unwelcome, answer.
pub async fn find_at_shop(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path((id, source)): Path<(u64, String)>,
) -> Result<Json<ShopFind>, AppError> {
    // Waitrose can't be reached from here — its bot-wall needs the Android app's
    // WebView, so the phone does that lookup and hands the results back to be
    // remembered. Rejecting it is honest; pretending to search it is not.
    if source != "asda" {
        return Err(AppError::BadRequest(format!(
            "{source} can't be searched from the server"
        )));
    }
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Some(barcode) = product.barcode.clone() else {
        return Err(AppError::BadRequest(
            "this product has no barcode to match on".to_string(),
        ));
    };

    if let Some(cached) = shop_cache::find_by_barcode(&app.pool, &source, &barcode).await? {
        return Ok(Json(ShopFind {
            hit: Some(cached_as_hit(cached)),
            from_cache: true,
        }));
    }

    let Some(query) = product
        .name
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return Err(AppError::BadRequest(
            "this product has no name to search by".to_string(),
        ));
    };
    let hits = asda::search(&app.http, query, 15).await?;
    remember_hits(&app.pool, &hits).await;
    Ok(Json(ShopFind {
        hit: asda::match_barcode(hits, &barcode),
        from_cache: false,
    }))
}

/// Which shop listing to pull, for `sync_listing`.
#[derive(serde::Deserialize)]
pub struct SyncListing {
    /// Registered source id — 'asda' today (see below).
    pub source: String,
    /// The source's id for the product (an Asda CIN).
    pub external_id: String,
}

/// POST /api/products/id/{id}/listings → pull this product's listing at a shop
/// and store what it says: the price (a new observation), the shop's lifestyle
/// tags, its pack size, and its clean name.
///
/// One operation for both "attach this shop" and "refresh it" — they differ only
/// in whether the listing already exists, and doing them by one idempotent path
/// means a refresh can never capture less than an attach did. Fetching shop-side
/// here (rather than accepting facts from the client) keeps the client from
/// asserting product facts, and lets the barcode guard below be enforced by the
/// server rather than trusted from the caller.
///
/// Asda only: its storefront search is a public API we can call from anywhere.
/// Waitrose needs the Android app's WebView to pass its bot-wall, so it has no
/// server-side fetch to offer here.
pub async fn sync_listing(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<SyncListing>,
) -> Result<Json<Product>, AppError> {
    if body.source != "asda" {
        return Err(AppError::BadRequest(format!(
            "cannot pull listings from {} server-side",
            body.source
        )));
    }
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Some(hit) = asda::fetch_by_id(&app.http, body.external_id.trim()).await? else {
        return Err(AppError::NotFound);
    };
    // The barcode is what makes this listing THIS product; a shop search is only
    // ever a relevance guess. Enforce the identity here so a mistaken (or
    // malicious) caller can't staple someone else's product onto this one.
    if hit.barcode.is_none() || hit.barcode != product.barcode {
        return Err(AppError::BadRequest(
            "that listing's barcode doesn't match this product".into(),
        ));
    }
    let updated = repo::upsert_external(
        &app.pool,
        "asda",
        &hit.external_id,
        hit.barcode.as_deref(),
        Some(&hit.name),
        hit.brand.as_deref(),
        None,
    )
    .await?;
    // Pack size only if we have none: OFF's quantity is the product's own, while
    // Asda's PACK_SIZE describes the pack it sells.
    if product.quantity_label.is_none()
        && let Some(q) = hit.quantity_label.as_deref()
    {
        repo::set_quantity_label(&app.pool, updated.id, q).await?;
    }
    // The shop's own lifestyle tags, kept apart from OFF's claims (migration
    // 0028) and merged on read.
    repo::replace_dietary(&app.pool, updated.id, &hit.dietary, "asda").await?;
    if let Some(price) = &hit.price
        && let Some(lid) = repo::listing_id(&app.pool, "asda", &hit.external_id).await?
    {
        repo::record_price(&app.pool, lid, price).await?;
    }
    // The picture is part of the identity we know about this product, not a
    // rotting figure — so pull it onto the product now, the same SSRF-gated
    // fetch the picker's import path and OFF lookups use. Only when the product
    // has none yet: an image we already have (from OFF, or a hand upload) is not
    // overwritten, matching the pack-size rule above. Best-effort — a failed
    // fetch just leaves the product image-less, never fails the attach.
    if !updated.has_image
        && let Some(url) = hit.image_url.as_deref().filter(|s| !s.is_empty())
        && let Some(src) = source::importable("asda")
        && !src.image_hosts.is_empty()
        && let Some((bytes, mime)) = off::fetch_image_from(url, src.image_hosts).await?
    {
        repo::set_image_by_id(&app.pool, updated.id, &bytes, &mime).await?;
    }
    tracing::info!(product = updated.id, cin = %hit.external_id, flags = hit.dietary.len(), "asda listing pulled");
    repo::get_by_id(&app.pool, updated.id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// GET /api/products/id/{id}/image → cached image bytes for a catalog row by id.
/// The barcodeless counterpart to /products/{barcode}/image (shop products have
/// no barcode to address the image by).
pub async fn image_by_id(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Response, AppError> {
    let (bytes, mime) = repo::get_image_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Other(e.into()))
}

/// GET /api/products/{barcode}/image → the cached image bytes.
pub async fn image(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<String>,
) -> Result<Response, AppError> {
    let (bytes, mime) = repo::get_image(&app.pool, &barcode)
        .await?
        .ok_or(AppError::NotFound)?;
    // Defense in depth for stored bytes served on our own origin: never let the
    // browser sniff them into something active, and sandbox the document if the
    // URL is opened directly (uploads are MIME-allowlisted, but old rows and
    // future regressions shouldn't become XSS).
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Other(e.into()))
}
