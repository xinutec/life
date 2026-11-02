//! Identity echo.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
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
