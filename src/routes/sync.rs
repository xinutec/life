//! The RxDB pull/push endpoints, one pair per collection.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;
use crate::sync::repo;
use crate::sync::types::{
    PullResponse, PushEntry, ShoppingDoc, TodoDoc, TodoLinkDoc, WellbeingDoc,
};

/// A push that does not decode is a 400 naming why, with nothing stored.
fn refused(e: JsonRejection) -> AppError {
    AppError::BadRequest(e.body_text())
}

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    /// The checkpoint; 0 pulls everything.
    #[serde(default)]
    since: u64,
    #[serde(default = "default_limit")]
    limit: u64,
}

fn default_limit() -> u64 {
    200
}

pub async fn pull_shopping(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse<ShoppingDoc>>, AppError> {
    let limit = q.limit.clamp(1, 1000);
    let res = repo::pull_shopping(&app.pool, &user.user_id, q.since, limit).await?;
    tracing::debug!(
        user = %user.user_id,
        since = q.since,
        returned = res.documents.len(),
        checkpoint = res.checkpoint.rev,
        "sync pull shopping"
    );
    Ok(Json(res))
}

/// Answers with the server's doc for each rejected (stale) change.
pub async fn push_shopping(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    payload: Result<Json<Vec<PushEntry<ShoppingDoc>>>, JsonRejection>,
) -> Result<Json<Vec<ShoppingDoc>>, AppError> {
    let Json(entries) = payload.map_err(refused)?;
    let pushed = entries.len();
    let conflicts = repo::push_shopping(&app.pool, &user.user_id, entries).await?;
    tracing::debug!(user = %user.user_id, pushed, conflicts = conflicts.len(), "sync push shopping");
    Ok(Json(conflicts))
}

pub async fn pull_todo(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse<TodoDoc>>, AppError> {
    let limit = q.limit.clamp(1, 1000);
    let res = repo::pull_todo(&app.pool, &user.user_id, q.since, limit).await?;
    tracing::debug!(
        user = %user.user_id,
        since = q.since,
        returned = res.documents.len(),
        checkpoint = res.checkpoint.rev,
        "sync pull todo"
    );
    Ok(Json(res))
}

pub async fn push_todo(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    payload: Result<Json<Vec<PushEntry<TodoDoc>>>, JsonRejection>,
) -> Result<Json<Vec<TodoDoc>>, AppError> {
    let Json(entries) = payload.map_err(refused)?;
    let pushed = entries.len();
    let conflicts = repo::push_todo(&app.pool, &user.user_id, entries).await?;
    tracing::debug!(user = %user.user_id, pushed, conflicts = conflicts.len(), "sync push todo");
    Ok(Json(conflicts))
}

pub async fn pull_todo_link(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse<TodoLinkDoc>>, AppError> {
    let limit = q.limit.clamp(1, 1000);
    let res = repo::pull_todo_link(&app.pool, &user.user_id, q.since, limit).await?;
    tracing::debug!(
        user = %user.user_id,
        since = q.since,
        returned = res.documents.len(),
        checkpoint = res.checkpoint.rev,
        "sync pull todo-link"
    );
    Ok(Json(res))
}

pub async fn push_todo_link(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    payload: Result<Json<Vec<PushEntry<TodoLinkDoc>>>, JsonRejection>,
) -> Result<Json<Vec<TodoLinkDoc>>, AppError> {
    let Json(entries) = payload.map_err(refused)?;
    let pushed = entries.len();
    let conflicts = repo::push_todo_link(&app.pool, &user.user_id, entries).await?;
    tracing::debug!(user = %user.user_id, pushed, conflicts = conflicts.len(), "sync push todo-link");
    Ok(Json(conflicts))
}

pub async fn pull_wellbeing(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse<WellbeingDoc>>, AppError> {
    let limit = q.limit.clamp(1, 1000);
    let res = repo::pull_wellbeing(&app.pool, &user.user_id, q.since, limit).await?;
    tracing::debug!(
        user = %user.user_id,
        since = q.since,
        returned = res.documents.len(),
        checkpoint = res.checkpoint.rev,
        "sync pull wellbeing"
    );
    Ok(Json(res))
}

pub async fn push_wellbeing(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    payload: Result<Json<Vec<PushEntry<WellbeingDoc>>>, JsonRejection>,
) -> Result<Json<Vec<WellbeingDoc>>, AppError> {
    let Json(entries) = payload.map_err(refused)?;
    let pushed = entries.len();
    let conflicts = repo::push_wellbeing(&app.pool, &user.user_id, entries).await?;
    tracing::debug!(user = %user.user_id, pushed, conflicts = conflicts.len(), "sync push wellbeing");
    Ok(Json(conflicts))
}
