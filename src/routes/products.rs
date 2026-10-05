//! Product catalogue HTTP surface.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use crate::error::AppError;
use crate::products::ids::{Barcode, ExternalId, ProductId};
use crate::products::ingest::{self, FactsUpdate, SourceAccount};
use crate::products::prices::PriceInput;
use crate::products::source::Source;
use crate::products::types::{Choice, DocKind, FieldChoice, ReconcileField};
use crate::products::types::{Product, ProductDetail, ProductListing, ProductReconciliation};
use crate::products::{asda, brandbank, off, repo, shop_cache};
use crate::purchases::repo as purchases_repo;
use crate::session::AuthUser;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: String,
}

/// GET /api/products/shop/asda?q= → a live Asda search. Every hit is remembered:
/// each carries its EAN, so one search teaches many barcode → CIN mappings.
pub async fn search_asda(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<asda::AsdaHit>>, AppError> {
    let hits = asda::search(&app.http, params.q.trim(), 15).await?;
    remember_hits(&app.pool, &hits).await;
    Ok(Json(hits))
}

/// Best-effort, but logged: a cache that never writes looks like one that works.
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

/// GET /api/products?q= → catalogue search by name or brand; no outside calls.
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

/// GET /api/products/{barcode} → the catalogue row, fetched from Open Food Facts
/// on a miss.
pub async fn lookup(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<Barcode>,
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
    let account = SourceAccount {
        source: Source::Off,
        external_id: ExternalId::from(&barcode),
        barcode: Some(barcode.clone()),
        name: found.name,
        brand: found.brand,
        quantity_label: found.quantity,
        url: None,
        image_url: found.image_url,
        raw_json: Some(found.raw),
        price: None,
        facts: FactsUpdate::Full(Box::new(found.facts)),
    };
    let picture = fetch_picture(None, &account).await;
    Ok(Json(repo::ingest(&app.pool, &account, picture).await?))
}

/// Fetched before the ingest, so no network call runs inside its transaction.
/// Best-effort: without one the product is stored pictureless.
async fn fetch_picture(
    current: Option<&Product>,
    account: &SourceAccount,
) -> Option<(Vec<u8>, String)> {
    let url = ingest::picture_to_fetch(current, account)?;
    off::fetch_image_from(url, account.source.image_hosts())
        .await
        .inspect_err(|e| tracing::warn!(%url, "product picture not fetched: {e:#}"))
        .ok()
        .flatten()
}

/// PUT /api/products/{barcode}/image → replace the picture with the body's bytes.
pub async fn set_image(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<Barcode>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // The declared mime only screens out obvious mistakes; the bytes decide.
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
    let Some(mime) = off::sniff_image_mime(&body) else {
        return Err(AppError::BadRequest(
            "the uploaded bytes are not a recognized image".into(),
        ));
    };
    repo::set_image(&app.pool, &barcode, &body, mime).await?;
    tracing::info!(%barcode, bytes = body.len(), %mime, "product image replaced");
    Ok(StatusCode::NO_CONTENT)
}

/// A shop's product, as the client normalised it.
#[derive(serde::Deserialize)]
pub struct ImportProduct {
    pub source: Source,
    pub external_id: ExternalId,
    pub name: String,
    pub brand: Option<String>,
    /// As the shop writes it ("400G").
    pub quantity_label: Option<String>,
    /// Merges the listing onto the canonical product for that barcode.
    pub barcode: Option<String>,
    /// Fetched server-side from an allowlisted host.
    pub image_url: Option<String>,
    pub price: Option<PriceInput>,
}

/// POST /api/products/import → upsert a shop's product, keyed on (source,
/// external_id).
pub async fn import(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(body): Json<ImportProduct>,
) -> Result<Json<Product>, AppError> {
    if !body.source.is_shop() {
        return Err(AppError::BadRequest(format!(
            "{} is not a shop to import from",
            body.source
        )));
    }
    let ext = &body.external_id;
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let brand = body
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    // Blank is "the shop didn't say"; anything else must be a real EAN.
    let barcode = body
        .barcode
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::parse::<Barcode>)
        .transpose()
        .map_err(AppError::BadRequest)?;
    let pack = body
        .quantity_label
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let account = SourceAccount {
        source: body.source,
        external_id: ext.clone(),
        barcode: barcode.clone(),
        name: Some(name.to_string()),
        brand: brand.map(str::to_string),
        quantity_label: pack.map(str::to_string),
        url: None,
        image_url: body.image_url.clone().filter(|s| !s.trim().is_empty()),
        raw_json: None,
        price: body.price.clone(),
        facts: FactsUpdate::None,
    };
    let current = match &barcode {
        Some(bc) => repo::get(&app.pool, bc).await?,
        None => repo::get_by_source_external(&app.pool, body.source, ext).await?,
    };
    let picture = fetch_picture(current.as_ref(), &account).await;
    let product = repo::ingest(&app.pool, &account, picture).await?;
    tracing::info!(source = %body.source, external_id = %ext, ?barcode, name, "product imported");
    Ok(Json(product))
}

/// GET /api/products/id/{id} → everything the product page shows.
pub async fn product_detail(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ProductId>,
) -> Result<Json<ProductDetail>, AppError> {
    Ok(Json(build_detail(&app.pool, &user.user_id, id).await?))
}

async fn build_detail(
    pool: &sqlx::MySqlPool,
    user_id: &str,
    id: ProductId,
) -> Result<ProductDetail, AppError> {
    let product = repo::get_by_id(pool, id).await?.ok_or(AppError::NotFound)?;
    // By id and barcode: a purchase from before this row existed is still one.
    let purchases = purchases_repo::history(
        pool,
        user_id,
        Some(id),
        product.barcode.as_ref().map(Barcode::as_str),
    )
    .await?;
    let (listings, prices, facts_by_source, fact_prefs, decisions, documents) = tokio::try_join!(
        repo::listings_for(pool, id),
        repo::latest_prices(pool, id),
        repo::facts_by_source(pool, id),
        repo::fact_source_prefs(pool, id),
        repo::field_decisions(pool, id),
        repo::documents_for(pool, id),
    )?;
    let facts = repo::merge_facts(&facts_by_source, &fact_prefs);
    let mut fields = repo::divergences(&product, &listings, &decisions);
    fields.extend(repo::fact_divergences(&facts_by_source, &fact_prefs));
    // Needs the raw listings' image URLs, so before they are mapped.
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
                .or_else(|| l.source.listing_url(&l.external_id)),
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
        purchases,
    })
}

/// POST /api/products/id/{id}/reconcile → settle where sources disagree; returns
/// the re-read detail.
pub async fn reconcile(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ProductId>,
    Json(body): Json<Vec<FieldChoice>>,
) -> Result<Json<ProductDetail>, AppError> {
    if repo::get_by_id(&app.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    // Adopting a picture fetches it, which must happen before the transaction.
    let (mut pictures, fields): (Vec<_>, Vec<_>) = body
        .into_iter()
        .partition(|c| c.field == ReconcileField::Picture);
    let total = pictures.len() + fields.len();
    let picture = match (pictures.pop(), pictures.is_empty()) {
        (None, _) => None,
        (Some(c), true) => Some(picture_choice(&app.pool, id, &c).await?),
        (Some(_), false) => {
            return Err(AppError::BadRequest("one picture choice at most".into()));
        }
    };
    repo::reconcile(&app.pool, id, picture, &fields).await?;
    tracing::info!(product = %id, decisions = total, "product reconciled");
    Ok(Json(build_detail(&app.pool, &user.user_id, id).await?))
}

/// A source's picture comes through the SSRF-gated fetch the import uses; `user`
/// is refused, as a picture is uploaded rather than typed.
async fn picture_choice(
    pool: &sqlx::MySqlPool,
    id: ProductId,
    c: &FieldChoice,
) -> Result<repo::PictureChoice, AppError> {
    let source = match c.choice {
        Choice::User => {
            return Err(AppError::BadRequest(
                "a picture is uploaded, not typed".into(),
            ));
        }
        Choice::Keep => return Ok(repo::PictureChoice::Keep),
        adopt => adopt
            .source()
            .ok_or_else(|| AppError::BadRequest(format!("{adopt} is not a source")))?,
    };
    let listings = repo::listings_for(pool, id).await?;
    let url = listings
        .iter()
        .find(|l| l.source == source)
        .and_then(|l| l.image_url.as_deref())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::BadRequest(format!("source {source} offers no picture to adopt"))
        })?;
    let hosts = source.image_hosts();
    if hosts.is_empty() {
        return Err(AppError::BadRequest(format!(
            "source {source} carries no adoptable picture"
        )));
    }
    let (bytes, mime) = off::fetch_image_from(url, hosts)
        .await?
        .ok_or_else(|| AppError::BadRequest("the source's picture host is not allowed".into()))?;
    Ok(repo::PictureChoice::Adopt {
        source,
        bytes,
        mime,
    })
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShopFind {
    pub hit: Option<asda::AsdaHit>,
    /// From memory rather than a fresh query: the UI says which.
    pub from_cache: bool,
    /// Whether the shop was asked: `hit: None` from a shop nobody asked means
    /// "not looked", which tells the phone to look itself.
    pub searched: bool,
}

/// A remembered listing as a hit: identity only, as price and flags would be
/// stale; attaching re-fetches them.
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
        raw: None,
    }
}

/// GET /api/products/id/{id}/find/{source} → does this shop carry the barcode?
/// Memory first; on a miss Asda is searched, matching on the barcode
/// ([`asda::match_barcode`]). Other shops answer `searched: false`.
pub async fn find_at_shop(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path((id, source)): Path<(ProductId, String)>,
) -> Result<Json<ShopFind>, AppError> {
    let source = match source.parse::<Source>() {
        Ok(s) if s.is_shop() => s,
        _ => return Err(AppError::BadRequest(format!("unknown shop: {source}"))),
    };
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Some(barcode) = product.barcode.clone() else {
        return Err(AppError::BadRequest(
            "this product has no barcode to match on".to_string(),
        ));
    };

    if let Some(cached) = shop_cache::find_by_barcode(&app.pool, source, &barcode).await? {
        return Ok(Json(ShopFind {
            hit: Some(cached_as_hit(cached)),
            from_cache: true,
            searched: false,
        }));
    }

    if source != Source::Asda {
        return Ok(Json(ShopFind {
            hit: None,
            from_cache: false,
            searched: false,
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
    let mut hit = asda::match_barcode(hits, &barcode);
    if hit.is_none()
        && let Some(second) = asda::fallback_query(query, product.brand.as_deref())
    {
        let hits = asda::search(&app.http, &second, 15).await?;
        remember_hits(&app.pool, &hits).await;
        hit = asda::match_barcode(hits, &barcode);
    }
    Ok(Json(ShopFind {
        hit,
        from_cache: false,
        searched: true,
    }))
}

/// POST /api/products/shop/{source}/listings → remember what a phone's WebView
/// saw at a shop the server cannot reach. Refuses anything that would poison the
/// barcode index.
pub async fn remember_seen(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(source): Path<String>,
    Json(seen): Json<Vec<shop_cache::SeenListing>>,
) -> Result<Json<Remembered>, AppError> {
    let listings = shop_cache::validate_seen(&source, &seen).map_err(AppError::BadRequest)?;
    for (from, to) in seen.iter().zip(&listings) {
        if from.image_url.is_some() && to.image_url.is_none() {
            tracing::warn!(
                %source, external_id = %to.external_id, url = ?from.image_url,
                "dropping a reported image URL: host is not allowlisted for this source"
            );
        }
    }
    shop_cache::remember(&app.pool, &listings).await?;
    tracing::info!(%source, count = listings.len(), "remembered what the client saw");
    Ok(Json(Remembered {
        remembered: listings.len(),
    }))
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Remembered {
    #[ts(type = "number")]
    pub remembered: usize,
}

#[derive(serde::Deserialize)]
pub struct SyncListing {
    pub source: Source,
    /// An Asda CIN.
    pub external_id: ExternalId,
}

/// POST /api/products/id/{id}/listings → fetch and store this product's listing
/// at Asda. Attach and refresh are one path; the server fetches, so the barcode
/// check below is enforced rather than trusted.
pub async fn sync_listing(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<ProductId>,
    Json(body): Json<SyncListing>,
) -> Result<Json<Product>, AppError> {
    if body.source != Source::Asda {
        return Err(AppError::BadRequest(format!(
            "cannot pull listings from {} server-side",
            body.source
        )));
    }
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Some(hit) = asda::fetch_by_id(&app.http, &body.external_id).await? else {
        return Err(AppError::NotFound);
    };
    // A search is a relevance guess; the barcode is identity.
    if hit.barcode.is_none() || hit.barcode != product.barcode {
        return Err(AppError::BadRequest(
            "that listing's barcode doesn't match this product".into(),
        ));
    }
    let account = SourceAccount {
        source: Source::Asda,
        external_id: hit.external_id.clone(),
        barcode: hit.barcode.clone(),
        name: Some(hit.name.clone()),
        brand: hit.brand.clone(),
        quantity_label: hit.quantity_label.clone(),
        url: None,
        image_url: hit.image_url.clone(),
        raw_json: hit.raw.as_ref().and_then(|v| serde_json::to_string(v).ok()),
        price: hit.price.clone(),
        facts: FactsUpdate::Dietary(hit.dietary.clone()),
    };
    let picture = fetch_picture(Some(&product), &account).await;
    let updated = repo::ingest(&app.pool, &account, picture).await?;
    tracing::info!(product = %updated.id, cin = %hit.external_id, flags = hit.dietary.len(), "asda listing pulled");
    Ok(Json(updated))
}

#[derive(serde::Deserialize)]
pub struct SubmitFacts {
    pub source: Source,
    /// The page's own barcode, which must be this product's.
    pub ean: Barcode,
    /// Parsed server-side; the client never asserts facts.
    pub blob: String,
}

/// POST /api/products/id/{id}/facts → store a shop page's facts blob, refused
/// unless the page's barcode is this product's. Returns the refreshed detail.
pub async fn submit_facts(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ProductId>,
    Json(body): Json<SubmitFacts>,
) -> Result<Json<ProductDetail>, AppError> {
    if body.source != Source::Asda {
        return Err(AppError::BadRequest(format!(
            "no facts parser for source {}",
            body.source
        )));
    }
    let product = repo::get_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if product.barcode.as_ref() != Some(&body.ean) {
        return Err(AppError::BadRequest(
            "that page's barcode doesn't match this product".into(),
        ));
    }
    // Kept verbatim first, so the WebView never has to fetch this page again.
    repo::upsert_document(&app.pool, id, Source::Asda, DocKind::Page, &body.blob).await?;
    let facts = brandbank::parse(&body.blob).map_err(|e| AppError::BadRequest(e.to_string()))?;
    repo::store_facts(&app.pool, id, &facts, Source::Asda).await?;
    tracing::info!(
        product = %id,
        bytes = body.blob.len(),
        nutrition = facts.nutrition.is_some(),
        allergens = facts.allergens.len(),
        dietary = facts.dietary.len(),
        "asda page fetched + stored"
    );
    build_detail(&app.pool, &user.user_id, id).await.map(Json)
}

/// GET /api/products/id/{id}/image → for products without a barcode.
pub async fn image_by_id(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<ProductId>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let (bytes, mime) = repo::get_image_by_id(&app.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    image_response(&headers, bytes, &mime)
}

pub async fn image(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(barcode): Path<Barcode>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let (bytes, mime) = repo::get_image(&app.pool, &barcode)
        .await?
        .ok_or(AppError::NotFound)?;
    image_response(&headers, bytes, &mime)
}

/// `no-cache` with an ETag: a replaced picture keeps its URL. Served from our own
/// origin, so never sniffed into anything active, and sandboxed if opened.
fn image_response(headers: &HeaderMap, bytes: Vec<u8>, mime: &str) -> Result<Response, AppError> {
    use sha2::{Digest, Sha256};
    let etag = format!("\"{}\"", hex::encode(&Sha256::digest(&bytes)[..16]));
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    let builder = Response::builder()
        .header(header::ETAG, &etag)
        .header(header::CACHE_CONTROL, "private, no-cache")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        );
    let res = if fresh {
        builder.status(StatusCode::NOT_MODIFIED).body(Body::empty())
    } else {
        builder
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(bytes))
    };
    res.map_err(|e| AppError::Other(e.into()))
}
