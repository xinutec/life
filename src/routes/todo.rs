//! To-dos for the case-file scripts (`dicom-scan`'s `push_life_todo.py`); the app
//! itself goes through sync.

use axum::Json;
use axum::extract::{Path, State};

use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;
use crate::todo::repo;
use crate::todo::types::{NewTodo, Todo, UpdateTodo};

pub async fn list(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<Todo>>, AppError> {
    Ok(Json(repo::list(&app.pool, &user.user_id).await?))
}

pub async fn create(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewTodo>,
) -> Result<Json<Todo>, AppError> {
    Ok(Json(repo::create(&app.pool, &user.user_id, body).await?))
}

pub async fn update(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<u64>,
    Json(body): Json<UpdateTodo>,
) -> Result<Json<Todo>, AppError> {
    repo::update(&app.pool, &user.user_id, id, body)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}
