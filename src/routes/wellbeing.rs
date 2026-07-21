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
use crate::wellbeing::suggest_store;

/// What the picker should show for this note, and whether a better answer is on
/// its way.
///
/// The reply is assembled from what is already known, never by waiting on a
/// model: generation happens on the Mac, out of band (see `suggest_store`). So
/// there are three honest answers, and this returns whichever applies:
///
/// - suggestions computed from exactly this wording — show them;
/// - suggestions computed from an earlier wording — show them marked `stale`,
///   because a note usually only drifts, and something close beats a blank space
///   while the new set is worked out;
/// - nothing yet — show nothing.
///
/// `pending` is set only when a worker has actually been seen recently. A picker
/// that claimed to be thinking with no model behind it would be lying, which is
/// worse than offering no suggestions at all.
pub async fn suggest_emotions(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SuggestEmotionsRequest>,
) -> Result<Json<SuggestEmotionsResponse>, AppError> {
    let nothing = || {
        Ok(Json(SuggestEmotionsResponse {
            suggestions: vec![],
            stale: false,
            pending: false,
            thinking_secs: None,
        }))
    };

    let note = body.note.trim();
    if note.is_empty() || body.candidates.is_empty() || body.ulid.is_empty() {
        return nothing();
    }

    let hash = suggest::note_hash(note);
    let cached = suggest_store::cached(&app.pool, &user.user_id, &body.ulid).await?;
    let fresh = cached.as_ref().is_some_and(|c| c.note_hash == hash);

    // Display-time filtering: the cache holds every valid token, and which of them
    // are worth offering depends on what is selected right now.
    let valid: HashSet<&str> = body.candidates.iter().map(|c| c.token.as_str()).collect();
    let already: HashSet<&str> = body.already.iter().map(|s| s.as_str()).collect();
    let suggestions = suggest::filter_suggestions(
        cached.map(|c| c.tokens).unwrap_or_default(),
        &valid,
        &already,
        suggest::MAX_SUGGESTIONS,
    );

    if fresh {
        return Ok(Json(SuggestEmotionsResponse {
            suggestions,
            stale: false,
            pending: false,
            thinking_secs: None,
        }));
    }

    // Queue the work even with no worker listening: the note is written now, and
    // whenever the Mac next wakes up the answer will be waiting the next time this
    // check-in is opened. Only the *promise* of an answer depends on a live worker.
    //
    // The picker asks again every couple of seconds while it waits, so the common
    // case here is "already queued" — check that first and build nothing.
    let queued = match suggest_store::pending_for(&app.pool, &user.user_id, &body.ulid, &hash)
        .await?
    {
        Some(queued) => queued,
        None => {
            let examples = suggest::fetch_examples(&app.pool, &user.user_id, suggest::MAX_EXAMPLES)
                .await
                .unwrap_or_default();
            let prompt = suggest::build_prompt(&body.candidates, &examples, note);
            let tokens: Vec<String> = body.candidates.iter().map(|c| c.token.clone()).collect();
            let queued = suggest_store::enqueue(
                &app.pool,
                &user.user_id,
                &body.ulid,
                &hash,
                &prompt,
                &tokens,
            )
            .await?;
            // Wake a worker already holding a poll open, so the note is picked up
            // now rather than at its next look.
            app.notify_job_queued();
            queued
        }
    };

    let pending = app.worker_alive();
    Ok(Json(SuggestEmotionsResponse {
        stale: !suggestions.is_empty(),
        suggestions,
        pending,
        thinking_secs: pending.then_some(u32::try_from(queued.thinking_secs).unwrap_or(u32::MAX)),
    }))
}
