//! Product lookup: cache-first, Open Food Facts on a miss; plus image serving.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use crate::error::AppError;
use crate::products::nutrition::ProductFacts;
use crate::products::prices::{PriceInput, ShopPrice};
use crate::products::{asda, off, repo, source, types::Product};
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
pub async fn search_asda(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<asda::AsdaHit>>, AppError> {
    Ok(Json(asda::search(&app.http, params.q.trim(), 15).await?))
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
        product.name.as_deref(),
    )
    .await?;
    // Store the nutrition/ingredients/allergens/dietary facts from the same OFF
    // response, attached to the canonical product.
    repo::store_facts(&app.pool, product.id, &found.facts, "off").await?;
    Ok(Json(product))
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

/// GET /api/products/id/{id}/prices → the latest price at each shop that lists
/// this product, cheapest first. Empty when no price has been observed yet.
pub async fn product_prices(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ShopPrice>>, AppError> {
    Ok(Json(repo::latest_prices(&app.pool, id).await?))
}

/// GET /api/products/id/{id}/facts → nutrition panel, ingredients, allergens,
/// and dietary flags for a product. All fields are optional/empty until the
/// product's barcode has been looked up against Open Food Facts.
pub async fn product_facts(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Json<ProductFacts>, AppError> {
    Ok(Json(repo::facts_for(&app.pool, id).await?))
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
