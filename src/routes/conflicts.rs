//! The sync-conflict log HTTP surface: report, list, resolve.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::conflicts::{ConflictEntry, NewConflict, repo};
use crate::error::{AppError, found_or_404};
use crate::session::AuthUser;
use crate::state::AppState;

/// GET /api/conflicts → unresolved conflicts, newest first.
pub async fn list(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<ConflictEntry>>, AppError> {
    Ok(Json(repo::list(&app.pool, &user.user_id).await?))
}

/// POST /api/conflicts → record a client-detected same-field conflict.
pub async fn create(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewConflict>,
) -> Result<StatusCode, AppError> {
    repo::create(&app.pool, &user.user_id, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/conflicts/{id}/resolve → 204, or 404 if unknown/already resolved.
pub async fn resolve(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
) -> Result<StatusCode, AppError> {
    found_or_404(repo::resolve(&app.pool, &user.user_id, id).await?)
}
