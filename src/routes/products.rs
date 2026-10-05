//! Product catalog HTTP surface: lookup, search, import, shop finds,
//! reconciliation, facts and images.

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

/// GET /api/products/shop/asda?q= → live name search against Asda (see
/// products::asda), the picker's explicit shop tier. A blank query is `[]` with
/// no outbound call. Every hit is remembered (products::shop_cache): each carries
/// its EAN, so one search teaches many barcode → CIN mappings.
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
    // Open Food Facts keys its own listing by the barcode, and its whole response
    // rides along verbatim, so nothing it sent is dropped.
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

/// The picture to store with an ingest, fetched before it so no network call
/// happens inside its transaction. Best-effort: a picture that cannot be fetched
/// is logged, and the product is stored without one.
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

/// PUT /api/products/{barcode}/image → replace the cached image with the raw
/// bytes in the request body (Content-Type names the mime). The frontend sends
/// the picked/pasted/dropped blob straight through, so there's no multipart to
/// parse. Body size is bounded by a per-route `DefaultBodyLimit` (see the router)
/// and re-checked here. Returns 204 on success.
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
    /// The shop this listing came from. Only a shop may be imported; an
    /// unknown id is refused by deserialization, before any of this runs.
    pub source: Source,
    /// Source-scoped id (e.g. a Waitrose lineNumber). Like `source`, a malformed
    /// one is refused by deserialization, before any of this runs.
    pub external_id: ExternalId,
    pub name: String,
    pub brand: Option<String>,
    /// The pack the shop sells, as the shop writes it ("400G"). Read as an
    /// amount on the way back out (see products::packsize), which is what lets
    /// stock linked from here start out knowing how much it holds. Optional:
    /// not every shop's search result carries one.
    pub quantity_label: Option<String>,
    /// The product's EAN, when the source knows it (Asda's IMAGE_ID, a Waitrose
    /// barCode). Reconciles this listing onto the canonical product for that
    /// barcode, so shop + Open Food Facts data merge into one product.
    pub barcode: Option<String>,
    /// Optional image on the source's CDN; fetched server-side, host-allowlisted.
    pub image_url: Option<String>,
    /// Optional price the source quoted; appended to the listing's price history.
    pub price: Option<PriceInput>,
    // No `category`: `products.category` is our ItemCategory, not a shop taxonomy.
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
    // A supplied barcode must be a real EAN before we key a canonical product on
    // it. Blank is absence rather than an error — clients send `""` for "the shop
    // didn't tell us" — but anything non-blank has to be one.
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
    // What the product holds now decides whether its picture is worth fetching.
    let current = match &barcode {
        Some(bc) => repo::get(&app.pool, bc).await?,
        None => repo::get_by_source_external(&app.pool, body.source, ext).await?,
    };
    let picture = fetch_picture(current.as_ref(), &account).await;
    // The answer carries the pack read FROM the label (`pack`), which the caller
    // fills a new stock row from the moment this returns.
    let product = repo::ingest(&app.pool, &account, picture).await?;
    tracing::info!(source = %body.source, external_id = %ext, ?barcode, name, "product imported");
    Ok(Json(product))
}

/// GET /api/products/id/{id} → everything the product page shows in one fetch:
/// the canonical product, its per-source listings (deep links resolved), the
/// latest price per shop (cheapest first), and its facts. Prices and facts are
/// empty until a shop quote / OFF lookup has provided them.
pub async fn product_detail(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ProductId>,
) -> Result<Json<ProductDetail>, AppError> {
    Ok(Json(build_detail(&app.pool, &user.user_id, id).await?))
}

/// Assemble the product-page aggregate for a product id (404 if it doesn't
/// exist). Shared by the detail GET and the reconcile POST, which answers with
/// the re-read detail.
async fn build_detail(
    pool: &sqlx::MySqlPool,
    user_id: &str,
    id: ProductId,
) -> Result<ProductDetail, AppError> {
    let product = repo::get_by_id(pool, id).await?.ok_or(AppError::NotFound)?;
    // By id AND barcode: a purchase made before this catalogue row existed, or
    // one whose link was corrected, is still this person's purchase.
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

/// POST /api/products/id/{id}/reconcile → settle field disagreements between the
/// product's sources and its canonical row. Each decision either adopts a
/// source's value or keeps the current one; either way the divergence is marked
/// settled so it won't resurface until a source's value changes. Returns the
/// re-read product detail (with the divergence list now updated).
pub async fn reconcile(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ProductId>,
    Json(body): Json<Vec<FieldChoice>>,
) -> Result<Json<ProductDetail>, AppError> {
    // 404 before touching anything if the product doesn't exist.
    if repo::get_by_id(&app.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    // The picture is chosen apart: adopting it re-fetches the source's image
    // through the SSRF gate, which happens here, before the transaction.
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

/// Resolve a picture reconcile choice. A source id fetches that source's
/// picture through the same SSRF-gated, no-redirect fetch the import path
/// uses. `user` is refused: a picture is uploaded (PUT .../image), not picked
/// as "our own" the way a typed name is.
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

/// The answer to "does this shop carry this product?".
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShopFind {
    /// The barcode-confirmed listing, if we have one.
    pub hit: Option<asda::AsdaHit>,
    /// Whether this came from memory rather than a fresh shop query. The UI says
    /// so: an answer we already had and one we just paid for are different
    /// things, and hiding which is which makes the cache unfalsifiable.
    pub from_cache: bool,
    /// Whether the shop itself was asked. Without this, `hit: None` would have to
    /// mean two opposite things: "we asked and this shop doesn't carry it" and
    /// "we've never looked". Only the server-searchable shops can produce the
    /// first; for a bot-walled shop a miss is always the second, and the phone —
    /// which CAN look — acts on the difference.
    pub searched: bool,
}

/// A remembered listing, shaped as a search hit.
///
/// Price and dietary flags are absent rather than stale: the cache keeps identity
/// (this barcode is this CIN), which doesn't rot. Attaching re-fetches the rest,
/// so this only has to let you confirm it's the right product.
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

/// GET /api/products/id/{id}/find/{source} → does this shop carry the barcode?
///
/// `shop_listings` answers first; on a miss the shop is asked and its whole
/// result remembered. Identity is the barcode, never the name
/// ([`asda::match_barcode`]). Only Asda can be searched from here: for Waitrose
/// a miss is `searched: false`, and the phone reports to `remember_seen`.
pub async fn find_at_shop(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Path((id, source)): Path<(ProductId, String)>,
) -> Result<Json<ShopFind>, AppError> {
    // A path segment is a client's string until it parses; after this line the
    // rest of the handler cannot be looking at a shop that doesn't exist.
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

    // Nothing in memory. Only a shop the server can query gets asked from here;
    // for the rest, saying "we haven't looked" is the whole answer, and it is the
    // answer the phone needs to know it should look itself.
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

/// POST /api/products/shop/{source}/listings → remember listings a phone's
/// WebView saw at a shop the server can't reach (`remember_hits`' mirror), so a
/// hunt's page loads are paid once. Forgiving in shape, but refuses anything that
/// would poison the barcode index. Returns how many rows were stored.
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

/// How many listings a report actually stored.
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Remembered {
    #[ts(type = "number")]
    pub remembered: usize,
}

/// Which shop listing to pull, for `sync_listing`.
#[derive(serde::Deserialize)]
pub struct SyncListing {
    /// The shop to pull from — 'asda' today (see below).
    pub source: Source,
    /// The source's id for the product (an Asda CIN).
    pub external_id: ExternalId,
}

/// POST /api/products/id/{id}/listings → fetch this product's listing at a shop
/// and store its price (a new observation), lifestyle tags, pack size and name.
///
/// Attach and refresh are one idempotent path, so a refresh never captures less
/// than an attach. The server fetches rather than accepting client facts, so the
/// barcode guard below is enforced, not trusted. Asda only: Waitrose has no
/// server-side fetch.
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
    // The barcode is what makes this listing THIS product; a shop search is only
    // ever a relevance guess. Enforce the identity here so a mistaken (or
    // malicious) caller can't staple someone else's product onto this one.
    if hit.barcode.is_none() || hit.barcode != product.barcode {
        return Err(AppError::BadRequest(
            "that listing's barcode doesn't match this product".into(),
        ));
    }
    // Asda's whole account: the structured fields plus the untouched record
    // (`raw_json`) on its own listing line, so nothing Asda sent is lost and every
    // field can stand as a candidate in reconciliation; its lifestyle tags, kept
    // apart from OFF's claims (migration 0028) and merged on read; its price.
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
    // The picture is identity, not a rotting figure, so it comes onto a product
    // that has none now. Best-effort: a failed fetch never fails the attach.
    let picture = fetch_picture(Some(&product), &account).await;
    let updated = repo::ingest(&app.pool, &account, picture).await?;
    tracing::info!(product = %updated.id, cin = %hit.external_id, flags = hit.dietary.len(), "asda listing pulled");
    Ok(Json(updated))
}

#[derive(serde::Deserialize)]
pub struct SubmitFacts {
    /// The shop whose page this is — 'asda' today.
    pub source: Source,
    /// The EAN the fetched page reported (Asda's `c_EAN_GTIN`), for the identity
    /// guard — this must be THIS product's barcode.
    pub ean: Barcode,
    /// The source's raw product-content blob (Asda's `c_BRANDBANK_JSON`), parsed
    /// server-side. The client never asserts the facts themselves.
    pub blob: String,
}

/// POST /api/products/id/{id}/facts → store facts only a shop's product page
/// carries (Asda's Brandbank blob). The page is behind Cloudflare, so the app's
/// WebView posts the raw blob and the server parses it; the client never
/// asserts interpreted facts. Refused unless the page's EAN is this product's.
/// Returns the refreshed detail.
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
    // The page's barcode is what makes its facts THIS product's. Enforce it here
    // so the WebView can't (by mistake or otherwise) post a different product's
    // page onto this one.
    if product.barcode.as_ref() != Some(&body.ean) {
        return Err(AppError::BadRequest(
            "that page's barcode doesn't match this product".into(),
        ));
    }
    // Keep the page's payload verbatim FIRST — so we hold it even if parsing finds
    // nothing (or a better parser wants it later), and never have to drive the
    // WebView through Cloudflare again for the same product.
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

/// GET /api/products/id/{id}/image → cached image bytes for a catalog row by id.
/// The barcodeless counterpart to /products/{barcode}/image (shop products have
/// no barcode to address the image by).
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

/// GET /api/products/{barcode}/image → the cached image bytes.
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

/// A stored image. `no-cache` with an `ETag` of its bytes: the URL stays the same
/// when the server replaces a picture, so each view revalidates, and an
/// unchanged picture costs a 304.
///
/// Stored bytes are served on our own origin, so never let the browser sniff
/// them into something active, and sandbox the document if the URL is opened
/// directly.
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
