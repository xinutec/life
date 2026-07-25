//! Wellbeing HTTP surface. The check-ins themselves reconcile through
//! `/api/sync/wellbeing` (see `sync::repo`); this holds the one derived,
//! online-only helper: emotion suggestions for the picker.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;
use crate::wellbeing::suggest::{
    self, EmotionCandidate, SuggestEmotionsRequest, SuggestEmotionsResponse, WarmEmotionsRequest,
};
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

    remember_vocabulary(&app, &user.user_id, &body.candidates).await;

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

    // A better answer is genuinely coming when a worker is on it: either one has
    // claimed this job and is generating right now — the strongest liveness signal
    // there is, and crucially the one that survives a long generation, during which
    // the blocked worker cannot poll and the "seen recently" clock goes stale — or
    // no one has claimed it yet but a worker polled recently and will. Keying
    // pending on `worker_alive()` alone made the picker give up ~90s into a
    // generation that takes ~100-145s and would have succeeded.
    let pending = queued.being_worked || app.worker_alive();
    Ok(Json(SuggestEmotionsResponse {
        stale: !suggestions.is_empty(),
        suggestions,
        pending,
        thinking_secs: pending.then_some(u32::try_from(queued.thinking_secs).unwrap_or(u32::MAX)),
    }))
}

/// Preload the model for a suggestion that is about to be asked for — fired when a
/// check-in's note starts being written. Building the *same* system prompt the
/// real request will use means the preload also warms that prompt's KV-cache
/// prefix, so the suggestion a moment later is a cache hit rather than a cold ~60s
/// load. Fire-and-forget: no worker, or a slow build, simply leaves the old
/// timing; the answer is still computed by the real request either way.
pub async fn warm_emotions(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<WarmEmotionsRequest>,
) -> Result<StatusCode, AppError> {
    if body.candidates.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }
    remember_vocabulary(&app, &user.user_id, &body.candidates).await;
    let examples = suggest::fetch_examples(&app.pool, &user.user_id, suggest::MAX_EXAMPLES)
        .await
        .unwrap_or_default();
    app.request_warm(suggest::build_system(&body.candidates, &examples));
    Ok(StatusCode::ACCEPTED)
}

/// Keep the vocabulary the picker just sent, so the rollover timer can rebuild
/// this prompt at midnight with nobody waiting (see `suggest_store`, 0038).
///
/// Deliberately infallible from the caller's side, and logged rather than
/// swallowed: this is a hint for tomorrow, and failing today's suggestion over it
/// would be the wrong trade — but a store that quietly never writes would look
/// exactly like one that works, and the only symptom would be a slow morning
/// nobody could explain.
async fn remember_vocabulary(app: &AppState, user_id: &str, candidates: &[EmotionCandidate]) {
    if let Err(e) = suggest_store::remember_vocabulary(&app.pool, user_id, candidates).await {
        tracing::warn!("could not remember the emotion vocabulary: {e:#}");
    }
}
