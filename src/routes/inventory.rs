//! Inventory HTTP surface: the location tree, items, and moves.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::AppError;
use crate::files::repo as files_repo;
use crate::files::types::{ItemFile, MAX_FILE_BYTES, sniff_mime};
use crate::inventory::consume::Taken;
use crate::inventory::repo;
use crate::inventory::types::{Item, ItemHistory, Location, NewItem, NewLocation, UseItem};
use crate::purchases::repo as purchases_repo;
use crate::purchases::types::{NewPurchase, Purchase};
use crate::session::AuthUser;
use crate::state::AppState;

pub async fn list_locations(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<Location>>, AppError> {
    Ok(Json(repo::list_locations(&app.pool, &user.user_id).await?))
}

pub async fn create_location(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewLocation>,
) -> Result<Json<Location>, AppError> {
    Ok(Json(
        repo::create_location(&app.pool, &user.user_id, body).await?,
    ))
}

pub async fn list_items(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<Item>>, AppError> {
    Ok(Json(repo::list_items(&app.pool, &user.user_id).await?))
}

pub async fn create_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewItem>,
) -> Result<Json<Item>, AppError> {
    Ok(Json(
        repo::create_item(&app.pool, &user.user_id, body).await?,
    ))
}

#[derive(Deserialize)]
pub struct MoveBody {
    pub location_id: Option<u64>,
}

pub async fn update_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<NewItem>,
) -> Result<Json<Item>, AppError> {
    repo::update_item(&app.pool, &user.user_id, id, body)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn delete_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<StatusCode, AppError> {
    if repo::delete_item(&app.pool, &user.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn delete_location(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<StatusCode, AppError> {
    if repo::delete_location(&app.pool, &user.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// GET /api/items/{id}/history → what has happened to this stock row, newest
/// first.
///
/// An unknown id answers with an empty list rather than a 404: the audit is
/// append-only and nothing guarantees a row has one — items added before the
/// table existed have none — so "no history" and "no such item" are the same
/// answer here, and distinguishing them would tell a caller whether somebody
/// else's item id exists.
pub async fn item_history(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Json<ItemHistory>, AppError> {
    let (entries, purchases) = tokio::try_join!(
        repo::item_history(&app.pool, &user.user_id, id),
        purchases_repo::for_item(&app.pool, &user.user_id, id),
    )?;
    Ok(Json(ItemHistory { entries, purchases }))
}

/// POST /api/items/{id}/purchases → record what this item cost, after the fact.
///
/// The buy-list flow already writes purchases, and it was the ONLY thing that
/// did — so anything you did not buy through the app could never have a price or
/// a date. That is most of a house: the dishwasher, the pans, everything owned
/// before the app existed. Measured 2026-09-03: one appliance in the inventory,
/// zero purchases against it.
///
/// ⚠ A bad price is a 400 here, where the buy flow logs it and carries on. That
/// asymmetry is deliberate and not an inconsistency: there, the purchase is a
/// note attached to something you are holding, and refusing the buy over a
/// mistyped price would lose the larger thing. Here the purchase IS the request,
/// so silently declining it would report success for a row that does not exist.
pub async fn record_purchase(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<NewPurchase>,
) -> Result<Json<Purchase>, AppError> {
    // Scoped through the item read, so somebody else's id is a 404 rather than a
    // purchase filed against a row they own.
    let item = repo::get_item(&app.pool, &user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let bought = purchases_repo::BoughtItem {
        id: item.id,
        product_id: item.product_id,
        barcode: item.barcode.as_deref(),
        name: &item.name,
        quantity: item.quantity,
        unit: item.unit.as_deref(),
    };
    let new_id = purchases_repo::record(&app.pool, &user.user_id, &bought, &body)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    // Read back rather than echo the request: `warranty_until` and the per-unit
    // rate are derived on read, and a hand-built reply would be the second place
    // that computes them.
    purchases_repo::for_item(&app.pool, &user.user_id, id)
        .await?
        .into_iter()
        .find(|p| p.id == new_id)
        .map(Json)
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("purchase {new_id} vanished after insert")))
}

/// DELETE /api/items/{id}/purchases/{purchase_id} → unmake a purchase.
///
/// A purchase is money, and the point of the table is that its numbers are
/// true — so a mistyped price or the wrong item has to be removable, not just
/// regrettable. Until this existed the only route was POST, and a typo in the
/// spending history was permanent.
///
/// It also makes the WRITE path testable. Without an inverse, exercising the
/// success path against production means leaving a fabricated number in the
/// record forever, so `record_purchase` shipped verified by its refusals alone.
pub async fn delete_purchase(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, purchase_id)): Path<(u64, u64)>,
) -> Result<StatusCode, AppError> {
    if purchases_repo::remove(&app.pool, &user.user_id, id, purchase_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// GET /api/items/{id}/files → what is attached to this item, newest first.
/// Metadata only; the bytes come from the download route.
pub async fn list_files(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ItemFile>>, AppError> {
    Ok(Json(
        files_repo::for_item(&app.pool, &user.user_id, id).await?,
    ))
}

/// POST /api/items/{id}/files → attach raw bytes. `X-File-Name` names it and
/// `X-Purchase-Id` optionally ties it to the purchase it is evidence of.
///
/// Raw body, not multipart: the client already holds a `File`/`Blob` and sends
/// it straight through, which is what the product-image route does and there is
/// nothing to parse. Bounded by a per-route `DefaultBodyLimit` and re-checked
/// here, because a limit enforced in only one of those places is a limit that
/// depends on the router still being wired the way you remember.
///
/// ⚠ The STORED mime is sniffed from the bytes, never the declared
/// `Content-Type`. These files are served back on our own origin, so bytes
/// riding in under an innocent-looking header would be stored XSS — the same
/// reason the image allowlist refuses SVG.
pub async fn add_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ItemFile>, AppError> {
    if repo::get_item(&app.pool, &user.user_id, id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("empty file".into()));
    }
    if body.len() > MAX_FILE_BYTES {
        return Err(AppError::BadRequest("file exceeds 10 MiB".into()));
    }
    let Some(mime) = sniff_mime(&body) else {
        return Err(AppError::BadRequest(
            "only images and PDFs can be attached, and the bytes are neither".into(),
        ));
    };
    let name = header_str(&headers, "x-file-name").unwrap_or("attachment");
    // A purchase id that is not a number is a client bug, not a reason to file
    // the receipt against nothing — say so rather than silently unlinking it.
    let purchase_id =
        match header_str(&headers, "x-purchase-id") {
            None => None,
            Some(raw) => Some(raw.parse::<u64>().map_err(|_| {
                AppError::BadRequest(format!("X-Purchase-Id is not a number: {raw}"))
            })?),
        };
    let new_id =
        files_repo::add(&app.pool, &user.user_id, id, purchase_id, name, mime, &body).await?;
    // Read the metadata back rather than assembling it here, so the created_at
    // the client sees is the one the database wrote.
    files_repo::for_item(&app.pool, &user.user_id, id)
        .await?
        .into_iter()
        .find(|f| f.id == new_id)
        .map(Json)
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("file {new_id} vanished after insert")))
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// GET /api/items/{id}/files/{file_id} → the bytes.
///
/// `Content-Disposition: attachment` on everything, including images. These are
/// arbitrary user uploads served from the app's own origin; making the browser
/// download rather than render them is what stops a crafted file being
/// interpreted in our security context.
pub async fn get_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, file_id)): Path<(u64, u64)>,
) -> Result<Response, AppError> {
    let (name, mime, bytes) = files_repo::read(&app.pool, &user.user_id, id, file_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // The filename is user text and goes into a header: strip anything that
    // could end the quoted string or inject a second header line.
    let safe: String = name
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\r' | '\n'))
        .take(120)
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// DELETE /api/items/{id}/files/{file_id} → detach and destroy.
pub async fn delete_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, file_id)): Path<(u64, u64)>,
) -> Result<StatusCode, AppError> {
    if files_repo::remove(&app.pool, &user.user_id, id, file_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn move_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Item>, AppError> {
    repo::move_item(&app.pool, &user.user_id, id, body.location_id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// POST /api/items/{id}/use → take an amount out of a stock row.
///
/// Returns the item as it now stands. A quantity the row can't be measured
/// against is a 400 that says which unit it *is* in, rather than a silent
/// no-op: the whole value of this is that the number in the cupboard stays
/// true, and quietly declining to change it would undermine that as surely as
/// changing it wrongly would.
pub async fn use_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<UseItem>,
) -> Result<Json<Item>, AppError> {
    if !body.quantity.is_finite() || body.quantity <= 0.0 {
        return Err(AppError::BadRequest(
            "how much did you use? give a positive amount".into(),
        ));
    }
    let (outcome, item) = repo::use_item(
        &app.pool,
        &user.user_id,
        id,
        body.quantity,
        body.unit.as_deref(),
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let item = item.ok_or(AppError::NotFound)?;
    match outcome {
        Taken::UnitMismatch => {
            return Err(AppError::BadRequest(match item.unit.as_deref() {
                Some(u) => format!(
                    "that is measured in {u}, so I can't take {} off it",
                    body.quantity
                ),
                None => "that doesn't have a unit to measure against".into(),
            }));
        }
        Taken::Untracked => {
            return Err(AppError::BadRequest(
                "that item doesn't track a quantity, so there's nothing to take from".into(),
            ));
        }
        Taken::Emptied { short } => tracing::info!(
            item = id, used = body.quantity, %short,
            "used more than the cupboard knew about — emptied"
        ),
        Taken::Left(left) => tracing::info!(item = id, used = body.quantity, left, "used"),
    }
    Ok(Json(item))
}
