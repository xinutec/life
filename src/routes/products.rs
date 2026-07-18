//! Product lookup: cache-first, Open Food Facts on a miss; plus image serving.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use crate::error::AppError;
use crate::products::prices::PriceInput;
use crate::products::types::{Product, ProductDetail, ProductListing, ProductReconciliation};
use crate::products::{asda, brandbank, off, repo, shop_cache, source};
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
    // barcode is Open Food Facts' own id for it), carrying OFF's own account of
    // the product so it stands as a candidate in any later reconciliation.
    repo::upsert_listing(
        &app.pool,
        product.id,
        "off",
        &barcode,
        &repo::ListingFields {
            raw_name: found.name.as_deref(),
            brand: found.brand.as_deref(),
            quantity_label: found.quantity.as_deref(),
            image_url: found.image_url.as_deref(),
            // OFF's whole response verbatim — the same lossless capture the Asda
            // search hit gets, so nothing OFF sent is dropped.
            raw_json: Some(&found.raw),
            ..Default::default()
        },
    )
    .await?;
    // Store the nutrition/ingredients/allergens/dietary facts from the same OFF
    // response, attached to the canonical product.
    repo::store_facts(&app.pool, product.id, &found.facts, "off").await?;
    // Seed the canonical name from the best-ranked source if the product had
    // none yet (fill-if-empty). It never switches an existing name: a source
    // that disagrees is surfaced as a divergence to approve, not applied here.
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
        &repo::ListingFields {
            raw_name: Some(name),
            brand,
            image_url: body.image_url.as_deref().filter(|s| !s.is_empty()),
            ..Default::default()
        },
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
        repo::set_image_provenance(&app.pool, product.id, &body.source).await?;
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
    Ok(Json(build_detail(&app.pool, id).await?))
}

/// Assemble the product-page aggregate for a product id (404 if it doesn't
/// exist). Shared by the detail GET and the reconcile POST, which answers with
/// the re-read detail.
async fn build_detail(pool: &sqlx::MySqlPool, id: u64) -> Result<ProductDetail, AppError> {
    let product = repo::get_by_id(pool, id).await?.ok_or(AppError::NotFound)?;
    let (listings, prices, facts_by_source, fact_prefs, decisions, documents) = tokio::try_join!(
        repo::listings_for(pool, id),
        repo::latest_prices(pool, id),
        repo::facts_by_source(pool, id),
        repo::fact_source_prefs(pool, id),
        repo::field_decisions(pool, id),
        repo::documents_for(pool, id),
    )?;
    // Merge the per-source facts to the one shown (honouring any source pick), and
    // build the diff to approve — the scalar disagreements plus the source-picked
    // facts (nutrition, ingredients) that genuinely differ.
    let facts = repo::merge_facts(&facts_by_source, &fact_prefs);
    let mut fields = repo::divergences(&product, &listings, &decisions);
    fields.extend(repo::fact_divergences(&facts_by_source, &fact_prefs));
    // The picture reconciles by provenance, not value (see picture_divergence):
    // needs the raw listings (their image_url), so compute it before mapping them.
    if let Some(pd) = repo::picture_divergence(&product, &listings, &decisions) {
        fields.push(pd);
    }
    let reconciliation = ProductReconciliation { fields };
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
    Ok(ProductDetail {
        product,
        listings,
        prices,
        facts,
        facts_by_source,
        reconciliation,
        documents,
    })
}

/// One field's decision, as the reconcile UI sends it: adopt a source's value
/// (`choice` = source id), keep the current one (`choice` = "keep"), or set our
/// own typed value (`choice` = "user", with `value`).
#[derive(serde::Deserialize)]
pub struct ReconcileChoice {
    pub field: String,
    pub choice: String,
    /// The typed value, when `choice` == "user".
    #[serde(default)]
    pub value: Option<String>,
}

/// POST /api/products/id/{id}/reconcile → settle field disagreements between the
/// product's sources and its canonical row. Each decision either adopts a
/// source's value or keeps the current one; either way the divergence is marked
/// settled so it won't resurface until a source's value changes. Returns the
/// re-read product detail (with the divergence list now updated).
pub async fn reconcile(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<Vec<ReconcileChoice>>,
) -> Result<Json<ProductDetail>, AppError> {
    // 404 before touching anything if the product doesn't exist.
    if repo::get_by_id(&app.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    // The picture reconciles differently: adopting it means re-fetching the
    // source's image through the SSRF gate (an I/O concern the route owns), not a
    // value copy. Split it out and handle it here; the rest is a plain DB reconcile.
    let (picture, scalar): (Vec<_>, Vec<_>) = body
        .into_iter()
        .partition(|c| c.field == repo::PICTURE_FIELD);
    let total = picture.len() + scalar.len();
    for c in &picture {
        apply_picture_choice(&app.pool, id, c).await?;
    }
    let choices: Vec<repo::FieldChoice> = scalar
        .into_iter()
        .map(|c| repo::FieldChoice {
            field: c.field,
            choice: c.choice,
            value: c.value,
        })
        .collect();
    repo::reconcile(&app.pool, id, &choices)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    tracing::info!(product = id, decisions = total, "product reconciled");
    Ok(Json(build_detail(&app.pool, id).await?))
}

/// Apply a picture reconcile choice. `keep` just settles the divergence; a source
/// id adopts that source's picture — re-fetching its bytes through the same
/// SSRF-gated, no-redirect fetch the import path uses, then recording the new
/// provenance. `user` is rejected: a picture is uploaded (PUT .../image), not
/// picked as "our own" the way a typed name is.
async fn apply_picture_choice(
    pool: &sqlx::MySqlPool,
    id: u64,
    c: &ReconcileChoice,
) -> Result<(), AppError> {
    if c.choice == repo::USER {
        return Err(AppError::BadRequest(
            "a picture is uploaded, not typed".into(),
        ));
    }
    if c.choice != repo::KEEP {
        let listings = repo::listings_for(pool, id).await?;
        let url = listings
            .iter()
            .find(|l| l.source == c.choice)
            .and_then(|l| l.image_url.as_deref())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::BadRequest(format!("source {} offers no picture to adopt", c.choice))
            })?;
        let hosts = source::image_hosts(&c.choice).filter(|h| !h.is_empty());
        let Some(hosts) = hosts else {
            return Err(AppError::BadRequest(format!(
                "source {} carries no adoptable picture",
                c.choice
            )));
        };
        let (bytes, mime) = off::fetch_image_from(url, hosts).await?.ok_or_else(|| {
            AppError::BadRequest("the source's picture host is not allowed".into())
        })?;
        repo::set_image_by_id(pool, id, &bytes, &mime).await?;
        repo::set_image_provenance(pool, id, &c.choice).await?;
    }
    // Settle AFTER any change, so the recorded set reflects the new provenance.
    repo::settle_picture(pool, id).await?;
    Ok(())
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
        // A remembered hit is identity only; the full record is re-fetched on
        // attach (`fetch_by_id`), which is what carries `raw` to storage.
        raw: None,
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
    // Store Asda's whole account of the product on its own listing line: the
    // structured fields plus the untouched record (`raw_json`), so nothing Asda
    // sent is lost and every field can stand as a candidate in reconciliation.
    let raw_json = hit.raw.as_ref().and_then(|v| serde_json::to_string(v).ok());
    let updated = repo::upsert_external(
        &app.pool,
        "asda",
        &hit.external_id,
        hit.barcode.as_deref(),
        &repo::ListingFields {
            raw_name: Some(&hit.name),
            brand: hit.brand.as_deref(),
            quantity_label: hit.quantity_label.as_deref(),
            image_url: hit.image_url.as_deref(),
            raw_json: raw_json.as_deref(),
            ..Default::default()
        },
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
        repo::set_image_provenance(&app.pool, updated.id, "asda").await?;
    }
    tracing::info!(product = updated.id, cin = %hit.external_id, flags = hit.dietary.len(), "asda listing pulled");
    repo::get_by_id(&app.pool, updated.id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[derive(serde::Deserialize)]
pub struct SubmitFacts {
    /// Registered source id — 'asda' today.
    pub source: String,
    /// The EAN the fetched page reported (Asda's `c_EAN_GTIN`), for the identity
    /// guard — this must be THIS product's barcode.
    pub ean: String,
    /// The source's raw product-content blob (Asda's `c_BRANDBANK_JSON`), parsed
    /// server-side. The client never asserts the facts themselves.
    pub blob: String,
}

/// POST /api/products/id/{id}/facts → store facts a shop's product PAGE carries
/// but its API doesn't: Asda's Brandbank nutrition, ingredients, allergens and
/// dietary claims. The page sits behind Cloudflare, so a server fetch can't reach
/// it (unlike `sync_listing`'s Algolia call); the client's hidden WebView fetches
/// the raw blob and posts it here, and the SERVER parses it — the client asserts
/// the raw page content, never the interpreted facts. Gated on the page's own EAN
/// matching this product, the same identity discipline `sync_listing` enforces by
/// barcode, so a mistaken or malicious caller can't staple another product's facts
/// on. Returns the refreshed detail.
pub async fn submit_facts(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<SubmitFacts>,
) -> Result<Json<ProductDetail>, AppError> {
    if body.source != "asda" {
        return Err(AppError::BadRequest(format!(
            "no facts parser for source {}",
            body.source
        )));
    }
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // The page's barcode is what makes its facts THIS product's. Enforce it here
    // so the WebView can't (by mistake or otherwise) post a different product's
    // page onto this one.
    let ean = body.ean.trim();
    if ean.is_empty() || product.barcode.as_deref() != Some(ean) {
        return Err(AppError::BadRequest(
            "that page's barcode doesn't match this product".into(),
        ));
    }
    // Keep the page's payload verbatim FIRST — so we hold it even if parsing finds
    // nothing (or a better parser wants it later), and never have to drive the
    // WebView through Cloudflare again for the same product.
    repo::upsert_document(&app.pool, id, "asda", "page", &body.blob).await?;
    let facts = brandbank::parse(&body.blob).map_err(|e| AppError::BadRequest(e.to_string()))?;
    repo::store_facts(&app.pool, id, &facts, "asda").await?;
    tracing::info!(
        product = id,
        bytes = body.blob.len(),
        nutrition = facts.nutrition.is_some(),
        allergens = facts.allergens.len(),
        dietary = facts.dietary.len(),
        "asda page fetched + stored"
    );
    build_detail(&app.pool, id).await.map(Json)
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
