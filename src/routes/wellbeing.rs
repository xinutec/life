//! Emotion suggestions for the picker; check-ins themselves go through sync.

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

/// Suggestions for this note from what is already known, never waiting on a
/// model: this wording's set, else an earlier wording's marked `stale`, else
/// none. `pending` only with a worker seen recently.
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

    // Queued even with no worker, so the answer waits for the next open. Usually
    // already queued: the picker asks every couple of seconds.
    let queued = match suggest_store::pending_for(&app.pool, &user.user_id, &body.ulid, &hash)
        .await?
    {
        Some(queued) => queued,
        None => {
            // Fail rather than build on no examples, which would answer worse
            // without saying so.
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
            // Wake a worker holding a poll open.
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

/// Preload the model while a note is being written, with the same system prompt
/// the real ask will use, so its cached prefix is warm. Best-effort.
pub async fn warm_emotions(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<WarmEmotionsRequest>,
) -> Result<StatusCode, AppError> {
    if body.candidates.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }
    remember_vocabulary(&app, &user.user_id, &body.candidates).await;
    // Without the examples the prompt would not match the real one.
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

/// So the midnight rollover can rebuild this prompt (0038). Best-effort, but
/// logged.
async fn remember_vocabulary(app: &AppState, user_id: &str, candidates: &[EmotionCandidate]) {
    if let Err(e) = suggest_store::remember_vocabulary(&app.pool, user_id, candidates).await {
        tracing::warn!("could not remember the emotion vocabulary: {e:#}");
    }
}
