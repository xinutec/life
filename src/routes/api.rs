//! Identity echo and the house scene.

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::error::AppError;
use crate::nextcloud::credentials::{self, LinkStatus};
use crate::session::AuthUser;
use crate::state::AppState;

/// A struct, so the TypeScript shape is generated.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Me {
    pub user_id: String,
    pub display_name: String,
    /// Nextcloud serves avatars publicly.
    pub avatar_url: String,
    pub nextcloud: LinkStatus,
}

/// GET /api/me → who am I, and is the calendar (CalDAV) link active.
pub async fn me(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Me>, AppError> {
    let nextcloud = credentials::status(&app.pool, &user.user_id).await?;
    Ok(Json(Me {
        avatar_url: format!("{}/avatar/{}/64", app.cfg.nc_base_url, user.user_id),
        user_id: user.user_id,
        display_name: user.display_name,
        nextcloud,
    }))
}

/// GET /api/house → scenes/house.json by default.
pub async fn house(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<Value>, AppError> {
    let text = tokio::fs::read_to_string(&app.cfg.house_scene)
        .await
        .map_err(|_| AppError::NotFound)?;
    let scene: Value = serde_json::from_str(&text).context("parsing house scene")?;
    Ok(Json(scene))
}
