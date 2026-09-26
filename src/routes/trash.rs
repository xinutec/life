//! The trash HTTP surface: list what's deleted, restore by kind + ref.

use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::error::{AppError, found_or_404};
use crate::session::AuthUser;
use crate::state::AppState;
use crate::trash::{TrashEntry, TrashKind, repo};

/// GET /api/trash → everything deleted, newest first.
pub async fn list(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<TrashEntry>>, AppError> {
    Ok(Json(repo::list(&app.pool, &user.user_id).await?))
}

/// POST /api/trash/{kind}/{ref}/restore → 204, or 404 if there was nothing to
/// restore (unknown ref, not deleted, not this user's).
pub async fn restore(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path((kind, r)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let kind = TrashKind::from_str(&kind).map_err(AppError::BadRequest)?;
    found_or_404(repo::restore(&app.pool, &user.user_id, kind, &r).await?)
}
