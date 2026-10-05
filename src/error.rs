use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not authenticated")]
    Unauthorized,

    #[error("not found")]
    NotFound,

    #[error("{0}")]
    BadRequest(String),

    #[error("nextcloud not linked")]
    NcNotLinked,

    #[error("nextcloud app password no longer valid — relink required")]
    NcReauthRequired,

    /// Outside life, so 502, and the text is shown: it is the user's to act on.
    #[error("{0}")]
    Upstream(String),

    /// 500 with a generic body; the detail is logged.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Other(e.into())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Other(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::NcNotLinked => (StatusCode::CONFLICT, self.to_string()),
            AppError::NcReauthRequired => (StatusCode::CONFLICT, self.to_string()),
            AppError::Upstream(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::Other(e) => {
                tracing::error!("internal error: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

/// 204 when the row existed to act on, else 404.
pub fn found_or_404(found: bool) -> Result<StatusCode, AppError> {
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
