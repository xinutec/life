//! Inventory HTTP surface: the location tree, items, and moves.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::error::AppError;
use crate::inventory::consume::Taken;
use crate::inventory::repo;
use crate::inventory::types::{Item, Location, NewItem, NewLocation, UseItem};
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
