//! Wellbeing HTTP surface. The check-ins themselves reconcile through
//! `/api/sync/wellbeing` (see `sync::repo`); this holds the one derived,
//! online-only helper: emotion suggestions for the picker.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;

use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;
use crate::wellbeing::suggest::{self, SuggestEmotionsRequest, SuggestEmotionsResponse};

/// Rank the app's feelings against a check-in note so the picker can offer the
/// best matches first. Online-only and best-effort: with no API key configured,
/// an empty note, or no candidates it returns `{ suggestions: [] }` — a 200, not
/// an error — so the picker falls back to the plain wheel and never blocks on the
/// network. Every returned token is validated against the request's own candidate
/// list, so a hallucinated feeling can never reach the client.
pub async fn suggest_emotions(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(body): Json<SuggestEmotionsRequest>,
) -> Result<Json<SuggestEmotionsResponse>, AppError> {
    let note = body.note.trim();
    let Some(api_key) = app.cfg.anthropic_api_key.as_deref() else {
        return Ok(Json(SuggestEmotionsResponse {
            suggestions: vec![],
        }));
    };
    if note.is_empty() || body.candidates.is_empty() {
        return Ok(Json(SuggestEmotionsResponse {
            suggestions: vec![],
        }));
    }

    let raw = suggest::request_suggestions(&app.http, api_key, note, &body.candidates).await?;

    let valid: HashSet<&str> = body.candidates.iter().map(|c| c.token.as_str()).collect();
    let already: HashSet<&str> = body.already.iter().map(|s| s.as_str()).collect();
    let suggestions = suggest::filter_suggestions(raw, &valid, &already, suggest::MAX_SUGGESTIONS);
    Ok(Json(SuggestEmotionsResponse { suggestions }))
}
