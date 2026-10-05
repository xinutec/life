//! Inventory HTTP surface: the location tree, items, and moves.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::{AppError, found_or_404};
use crate::files::repo as files_repo;
use crate::files::types::{FileId, ItemFile, MAX_FILE_BYTES, sniff_mime};
use crate::inventory::consume::Taken;
use crate::inventory::repo;
use crate::inventory::types::{
    Item, ItemHistory, ItemId, Location, LocationId, NewItem, NewLocation, UseItem,
};
use crate::products::ids::ProductId;
use crate::purchases::repo as purchases_repo;
use crate::purchases::types::{NewPurchase, Purchase, PurchaseId};
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
    own_location(&app, &user.user_id, body.parent_id).await?;
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
    own_location(&app, &user.user_id, body.location_id).await?;
    Ok(Json(
        repo::create_item(&app.pool, &user.user_id, body).await?,
    ))
}

/// A client-sent location must be one of this user's live ones.
async fn own_location(
    app: &AppState,
    user_id: &str,
    id: Option<LocationId>,
) -> Result<(), AppError> {
    if repo::is_own_location(&app.pool, user_id, id).await? {
        Ok(())
    } else {
        Err(AppError::BadRequest("no such location".into()))
    }
}

#[derive(Deserialize)]
pub struct MoveBody {
    pub location_id: Option<LocationId>,
}

pub async fn update_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
    Json(body): Json<NewItem>,
) -> Result<Json<Item>, AppError> {
    own_location(&app, &user.user_id, body.location_id).await?;
    repo::update_item(&app.pool, &user.user_id, id, body)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn delete_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<StatusCode, AppError> {
    found_or_404(repo::delete_item(&app.pool, &user.user_id, id).await?)
}

pub async fn delete_location(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<LocationId>,
) -> Result<StatusCode, AppError> {
    found_or_404(repo::delete_location(&app.pool, &user.user_id, id).await?)
}

/// GET /api/items/{id}/history → newest first. An unknown id gets an empty list,
/// not a 404, which would reveal whether somebody else's id exists.
pub async fn item_history(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<ItemHistory>, AppError> {
    let (entries, purchases) = tokio::try_join!(
        repo::item_history(&app.pool, &user.user_id, id),
        purchases_repo::for_item(&app.pool, &user.user_id, id),
    )?;
    Ok(Json(ItemHistory { entries, purchases }))
}

/// POST /api/items/{id}/purchases → record what this item cost. A bad price is a
/// 400 here: unlike in the buy flow, the purchase is the whole request.
pub async fn record_purchase(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
    Json(body): Json<NewPurchase>,
) -> Result<Json<Purchase>, AppError> {
    // Through the item read, so somebody else's id is a 404.
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
    // Read back: `warranty_until` and the rate are derived on read.
    purchases_repo::for_item(&app.pool, &user.user_id, id)
        .await?
        .into_iter()
        .find(|p| p.id == new_id)
        .map(Json)
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("purchase {new_id} vanished after insert")))
}

/// DELETE /api/items/{id}/purchases/{purchase_id} → to the trash.
pub async fn delete_purchase(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, purchase_id)): Path<(ItemId, PurchaseId)>,
) -> Result<StatusCode, AppError> {
    found_or_404(purchases_repo::remove(&app.pool, &user.user_id, id, purchase_id).await?)
}

/// GET /api/items/{id}/files → metadata, newest first.
pub async fn list_files(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<ItemFile>>, AppError> {
    Ok(Json(
        files_repo::for_item(&app.pool, &user.user_id, id).await?,
    ))
}

/// POST /api/items/{id}/files → attach the raw body, named by `X-File-Name` and
/// optionally tied to `X-Purchase-Id`. The stored mime is sniffed, never the
/// declared one: trusting it would allow stored XSS on our origin.
pub async fn add_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
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
    // A malformed purchase id is refused, not silently dropped.
    let purchase_id =
        match header_str(&headers, "x-purchase-id") {
            None => None,
            Some(raw) => Some(raw.parse::<u64>().map(PurchaseId::from).map_err(|_| {
                AppError::BadRequest(format!("X-Purchase-Id is not a number: {raw}"))
            })?),
        };
    // Must be a live purchase of this item.
    if let Some(pid) = purchase_id
        && !purchases_repo::for_item(&app.pool, &user.user_id, id)
            .await?
            .iter()
            .any(|p| p.id == pid)
    {
        return Err(AppError::BadRequest(
            "that purchase is not one of this item's".into(),
        ));
    }
    let new_id =
        files_repo::add(&app.pool, &user.user_id, id, purchase_id, name, mime, &body).await?;
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

/// GET /api/items/{id}/files/{file_id} → the bytes, always as a download: user
/// uploads on our origin must not render in our security context.
pub async fn get_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, file_id)): Path<(ItemId, FileId)>,
) -> Result<Response, AppError> {
    let (name, mime, bytes) = files_repo::read(&app.pool, &user.user_id, id, file_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // User text in a header: nothing may end the quoted string or the line.
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

/// DELETE /api/items/{id}/files/{file_id} → to the trash.
pub async fn delete_file(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, file_id)): Path<(ItemId, FileId)>,
) -> Result<StatusCode, AppError> {
    found_or_404(files_repo::remove(&app.pool, &user.user_id, id, file_id).await?)
}

pub async fn move_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Item>, AppError> {
    own_location(&app, &user.user_id, body.location_id).await?;
    repo::move_item(&app.pool, &user.user_id, id, body.location_id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[derive(Debug, Deserialize)]
pub struct LowByIdentity {
    pub name: String,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub product_id: Option<ProductId>,
}

/// POST /api/items/low → the same, from the Buy list. 204 whether or not a
/// cupboard row matched.
pub async fn mark_low_by_identity(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<LowByIdentity>,
) -> Result<StatusCode, AppError> {
    repo::mark_low_matching(
        &app.pool,
        &user.user_id,
        &body.name,
        body.barcode.as_deref(),
        body.product_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/items/{id}/low → record that this is running out.
pub async fn mark_low(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<StatusCode, AppError> {
    found_or_404(repo::mark_low(&app.pool, &user.user_id, id).await?)
}

/// POST /api/items/{id}/use → take an amount out; returns the item. An amount in
/// another unit is a 400 naming the row's unit, never a silent no-op.
pub async fn use_item(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<ItemId>,
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
            item = %id, used = body.quantity, %short,
            "used more than the cupboard knew about — emptied"
        ),
        Taken::Left(left) => tracing::info!(item = %id, used = body.quantity, left, "used"),
    }
    Ok(Json(item))
}
