//! Buying a Buy-list row into the inventory, and where rows are sold.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::AppError;
use crate::inventory::types::Item;
use crate::products::coverage;
use crate::products::repo as product_repo;
use crate::purchases::repo as purchases_repo;
use crate::purchases::types::NewPurchase;
use crate::session::AuthUser;
use crate::shopping::repo;
use crate::shopping::types::ShoppingItem;
use crate::state::AppState;

pub async fn list(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<ShoppingItem>>, AppError> {
    Ok(Json(repo::list(&app.pool, &user.user_id).await?))
}

/// The price, if it was noted: buying must work with a full trolley.
#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
pub struct BuyRequest {
    #[serde(default)]
    pub purchase: Option<NewPurchase>,
}

/// POST /api/shopping/{id}/buy → turn a row into an unplaced item, in one
/// transaction (`shopping::repo::buy`), so a double tap 404s.
pub async fn buy(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    body: Option<Json<BuyRequest>>,
) -> Result<Json<Item>, AppError> {
    let item = repo::buy(&app.pool, &user.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;

    // After the item exists, and never failing the buy: a bad price is logged.
    if let Some(Json(BuyRequest {
        purchase: Some(ref p),
    })) = body
    {
        let bought = purchases_repo::BoughtItem {
            id: item.id,
            product_id: item.product_id,
            barcode: item.barcode.as_deref(),
            name: &item.name,
            quantity: item.quantity,
            unit: item.unit.as_deref(),
        };
        if let Err(e) = purchases_repo::record(&app.pool, &user.user_id, &bought, p).await {
            tracing::warn!(error = %e, item = %item.id, "purchase not recorded; the buy stands");
        }
    }
    Ok(Json(item))
}

/// POST /api/shopping/coverage → where each row is known to be sold, and the
/// latest prices, from memory only. Empty `sources` means unknown, not nowhere.
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
    let prices =
        product_repo::latest_prices_for(&app.pool, &coverage::product_ids(&queries)).await?;
    Ok(Json(coverage::combine(&queries, &attached, &seen, &prices)))
}
