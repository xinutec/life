//! Shopping-list HTTP surface, plus the buy→inventory conversion.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use ts_rs::TS;

use crate::error::AppError;
use crate::inventory::repo as inventory_repo;
use crate::inventory::types::{Item, NewItem};
use crate::products::coverage;
use crate::products::repo as product_repo;
use crate::purchases::repo as purchases_repo;
use crate::purchases::types::NewPurchase;
use crate::session::AuthUser;
use crate::shopping::repo;
use crate::shopping::types::{NewShoppingItem, ShoppingItem, UpdateShoppingItem};
use crate::state::AppState;

pub async fn list(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<ShoppingItem>>, AppError> {
    Ok(Json(repo::list(&app.pool, &user.user_id).await?))
}

pub async fn create(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewShoppingItem>,
) -> Result<Json<ShoppingItem>, AppError> {
    Ok(Json(repo::create(&app.pool, &user.user_id, body).await?))
}

pub async fn update(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<UpdateShoppingItem>,
) -> Result<Json<ShoppingItem>, AppError> {
    repo::update(&app.pool, &user.user_id, id, body)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn delete(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<StatusCode, AppError> {
    if repo::delete(&app.pool, &user.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// What may ride along with a buy: the price, if it was noted.
///
/// Optional because it must be, not because it is a nicety. Marking something
/// bought is the gesture that empties the list, and it has to keep working with
/// a full trolley and one hand — a capture step that blocks it would be skipped,
/// and then the list stops being used at all. Recording nothing is a valid,
/// common answer.
#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
pub struct BuyRequest {
    #[serde(default)]
    pub purchase: Option<NewPurchase>,
}

/// POST /api/shopping/{id}/buy → turn a bought item into an inventory item
/// (unplaced) and remove it from the list. Returns the item. The row's own
/// `category` and `product_id` carry onto the item — the row knows what it is;
/// nothing is guessed here.
///
/// Ordering makes a double-tap idempotent: the soft-delete (guarded by
/// `rows_affected`) is the claim — only the request that actually tombstones the
/// row creates the inventory item; a concurrent duplicate 404s instead of
/// minting a second item. A crash between the two writes loses nothing
/// permanent (the shopping row is tombstoned, not gone).
pub async fn buy(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    body: Option<Json<BuyRequest>>,
) -> Result<Json<Item>, AppError> {
    let s = repo::get(&app.pool, &user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !repo::delete(&app.pool, &user.user_id, id).await? {
        return Err(AppError::NotFound); // already bought/deleted concurrently
    }
    let item = inventory_repo::create_item(
        &app.pool,
        &user.user_id,
        NewItem {
            name: s.name,
            category: s.category,
            quantity: s.quantity,
            unit: s.unit,
            expiry: None,
            location_id: None,
            barcode: s.barcode,
            product_id: s.product_id,
            // A buy-list row's name is a note to self ("cheese"), not a naming
            // of the thing that comes home. The catalogue outranks it, which is
            // what `None` asks for.
            name_source: None,
        },
    )
    .await?;

    // After the item exists, and never in a way that can fail the buy. The
    // purchase is a note about money; the item is the thing you are holding.
    // Losing the note is a small loss, and refusing the buy over it would be a
    // large one — so a bad price is reported and the buy still stands.
    if let Some(Json(BuyRequest {
        purchase: Some(ref p),
    })) = body
    {
        let bought = purchases_repo::BoughtItem {
            product_id: item.product_id,
            barcode: item.barcode.as_deref(),
            name: &item.name,
            quantity: item.quantity,
            unit: item.unit.as_deref(),
        };
        if let Err(e) = purchases_repo::record(&app.pool, &user.user_id, &bought, p).await {
            tracing::warn!(error = %e, item = item.id, "purchase not recorded; the buy stands");
        }
    }
    Ok(Json(item))
}

/// POST /api/shopping/coverage → where each of these rows is known to be sold.
///
/// Reads memory only: the shops that hold a listing for the row's product, plus
/// the shops a past query showed carrying its barcode. No outbound traffic, so a
/// whole list costs two queries and the shops nothing — which is the point, since
/// this runs every time the Buy list loads.
///
/// It answers "where is this SOLD", never "is it in stock": the freshest thing
/// here is a sighting from whenever someone last looked. An empty `sources` means
/// we know nothing about that row, not that nowhere sells it.
pub async fn coverage(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(queries): Json<Vec<coverage::CoverageQuery>>,
) -> Result<Json<Vec<coverage::RowCoverage>>, AppError> {
    if queries.len() > coverage::MAX_ROWS {
        return Err(AppError::BadRequest(format!(
            "at most {} rows per request",
            coverage::MAX_ROWS
        )));
    }
    let attached = product_repo::shops_holding(&app.pool, &coverage::product_ids(&queries)).await?;
    let seen = product_repo::shops_seen_carrying(&app.pool, &coverage::barcodes(&queries)).await?;
    Ok(Json(coverage::combine(&queries, &attached, &seen)))
}
