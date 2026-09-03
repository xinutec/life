//! Inventory HTTP surface: the location tree, items, and moves.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::error::AppError;
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
