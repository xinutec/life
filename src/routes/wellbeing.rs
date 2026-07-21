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
/// best matches first, personalised with a few-shot of the user's own past
/// taggings.
///
/// Best-effort throughout: with no model server configured, an empty note, no
/// candidates, or a server that's slow/down/unreachable, it returns
/// `{ suggestions: [] }` — a 200, not an error — so the picker falls back to the
/// plain wheel and the spinner clears. Every returned token is validated against
/// the request's own candidate list, so a hallucinated feeling can't reach the UI.
pub async fn suggest_emotions(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SuggestEmotionsRequest>,
) -> Result<Json<SuggestEmotionsResponse>, AppError> {
    let empty = || {
        Ok(Json(SuggestEmotionsResponse {
            suggestions: vec![],
        }))
    };

    let note = body.note.trim();
    // Return instantly when the feature is off, so no spinner ever flashes.
    let Some(base_url) = app.cfg.emotion_model_url.as_deref() else {
        return empty();
    };
    if note.is_empty() || body.candidates.is_empty() {
        return empty();
    }

    // Few-shot: teach the model THIS user's own calibration from their history.
    // A failed fetch just means a generic (unpersonalised) prompt, not an error.
    let examples = suggest::fetch_examples(&app.pool, &user.user_id, suggest::MAX_EXAMPLES)
        .await
        .unwrap_or_default();

    // A model that's slow, down, or unreachable must never fail the picker.
    let raw = match suggest::request_suggestions(
        &app.http,
        base_url,
        &app.cfg.emotion_model,
        note,
        &body.candidates,
        &examples,
    )
    .await
    {
        Ok(raw) => raw,
        Err(e) => {
            tracing::info!("emotion suggestions unavailable: {e:#}");
            return empty();
        }
    };

    let valid: HashSet<&str> = body.candidates.iter().map(|c| c.token.as_str()).collect();
    let already: HashSet<&str> = body.already.iter().map(|s| s.as_str()).collect();
    let suggestions = suggest::filter_suggestions(raw, &valid, &already, suggest::MAX_SUGGESTIONS);
    Ok(Json(SuggestEmotionsResponse { suggestions }))
}
