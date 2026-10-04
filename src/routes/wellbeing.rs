//! Wellbeing HTTP surface. The check-ins themselves reconcile through
//! `/api/sync/wellbeing` (see `sync::repo`); this holds the one derived,
//! online-only helper: emotion suggestions for the picker.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;
use crate::wellbeing::suggest::{
    self, EmotionCandidate, Progress, SuggestEmotionsRequest, SuggestEmotionsResponse,
    WarmEmotionsRequest,
};
use crate::wellbeing::suggest_store;

/// Suggestions for this note, from what is already known; generation happens
/// out of band on the Mac (`suggest_store`), so this never waits on a model.
/// Returns the set for exactly this wording, else the set for an earlier wording
/// marked `stale` (notes drift, and close beats blank), else nothing.
///
/// `pending` is set only if a worker was seen recently, so the picker never
/// claims to be thinking with no model behind it.
pub async fn suggest_emotions(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SuggestEmotionsRequest>,
) -> Result<Json<SuggestEmotionsResponse>, AppError> {
    let answer = |cached, progress| {
        Ok(Json(suggest::respond(
            cached,
            progress,
            &body.candidates,
            &body.already,
        )))
    };
    let note = body.note.trim();
    if note.is_empty() || body.candidates.is_empty() || body.ulid.is_empty() {
        return answer(Vec::new(), Progress::Answered);
    }

    remember_vocabulary(&app, &user.user_id, &body.candidates).await;

    let hash = suggest::note_hash(note);
    let cached = suggest_store::cached(&app.pool, &user.user_id, &body.ulid).await?;
    let fresh = cached.as_ref().is_some_and(|c| c.note_hash == hash);
    let tokens = cached.map(|c| c.tokens).unwrap_or_default();
    if fresh {
        return answer(tokens, Progress::Answered);
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
            // A failed read fails the ask, which the picker repeats. Building on
            // no examples instead would queue a prompt unlike the day's warmed
            // one, and answer worse without saying so.
            let examples =
                suggest::fetch_examples(&app.pool, &user.user_id, suggest::MAX_EXAMPLES).await?;
            let prompt = suggest::build_prompt(&body.candidates, &examples, note);
            let vocabulary: Vec<String> = body.candidates.iter().map(|c| c.token.clone()).collect();
            let queued = suggest_store::enqueue(
                &app.pool,
                &user.user_id,
                &body.ulid,
                &hash,
                &prompt,
                &vocabulary,
            )
            .await?;
            // Wake a worker already holding a poll open, so the note is picked up
            // now rather than at its next look.
            app.notify_job_queued();
            queued
        }
    };
    answer(
        tokens,
        Progress::Waiting {
            secs: queued.thinking_secs,
            being_worked: queued.being_worked,
            worker_alive: app.worker_alive(),
        },
    )
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
    // Without the examples there is nothing worth warming: a prompt built from
    // none is not the one the real ask will use, so its cached prefix would miss.
    match suggest::fetch_examples(&app.pool, &user.user_id, suggest::MAX_EXAMPLES).await {
        Ok(examples) => {
            app.request_warm(suggest::build_system(&body.candidates, &examples));
            Ok(StatusCode::ACCEPTED)
        }
        Err(e) => {
            tracing::warn!("not warming the emotion model: {e:#}");
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

/// Keep the vocabulary the picker just sent, so the rollover timer can rebuild
/// this prompt at midnight with nobody waiting (see `suggest_store`, 0038).
///
/// Infallible for the caller — a hint for tomorrow must not fail today's
/// suggestion — but logged, or a store that never writes looks like one that does.
async fn remember_vocabulary(app: &AppState, user_id: &str, candidates: &[EmotionCandidate]) {
    if let Err(e) = suggest_store::remember_vocabulary(&app.pool, user_id, candidates).await {
        tracing::warn!("could not remember the emotion vocabulary: {e:#}");
    }
}
