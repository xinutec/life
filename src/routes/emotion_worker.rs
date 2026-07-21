//! The channel the emotion-suggestion worker talks to.
//!
//! The model lives on the Mac, which is a deliberately one-way WireGuard peer:
//! it may open connections into the fleet, and nothing in the fleet may open one
//! toward it (the point being that a compromised server must not be able to reach
//! the machine holding the originals). So the fleet cannot call the model — the
//! worker calls in, asks for work, and posts the answer back. That inversion is
//! the whole reason this module exists.
//!
//! Waiting happens here rather than on the worker: [`next`] holds the request open
//! until a job appears or the poll window runs out. A worker that hammered a
//! plain "anything for me?" endpoint would either burn a request a second or make
//! you watch a note sit unread for the length of its sleep; holding the socket
//! costs nothing and gets a note picked up the moment you finish writing it.
//!
//! Authentication is a bearer token, not a session: the worker is a daemon, not a
//! browser, and it acts for no user — the jobs it is handed carry a prompt and
//! nothing that identifies whose check-in it belongs to.

use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;
use crate::wellbeing::suggest;
use crate::wellbeing::suggest_store;

/// How long a poll is held open before answering "nothing for you". Short enough
/// to sit well inside any proxy's idle timeout, long enough that the worker
/// spends its life waiting rather than reconnecting.
const POLL_WINDOW: Duration = Duration::from_secs(25);
/// Backstop re-check while a poll is held. A job queued by this process wakes the
/// waiter immediately; this only covers what that signal cannot see — a job left
/// behind by a previous pod, or queued by another one. Deliberately slow, since
/// its whole purpose is to be redundant.
const RECHECK: Duration = Duration::from_secs(5);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobOut {
    id: u64,
    /// `{ "system": …, "user": … }` — everything the model needs, and nothing else.
    prompt: serde_json::Value,
}

/// What the worker got back from the model. The raw text, not a parsed list: the
/// worker's only job is to run the model, and every decision about what that text
/// means — is it even JSON, are those real feelings — belongs on this side, where
/// it is tested and where the vocabulary is known.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultIn {
    #[serde(default)]
    content: Option<String>,
    /// Set instead of `content` when the model could not be run at all.
    #[serde(default)]
    error: Option<String>,
}

/// Bearer check. A missing token, a wrong one, and an unconfigured channel all
/// answer the same 401, so probing this endpoint reveals nothing about whether a
/// worker exists.
fn authorized(app: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(expected) = app.cfg.emotion_worker_token.as_deref() else {
        return Err(AppError::Unauthorized);
    };
    let given = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if given.is_empty() || given != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

/// Long-poll for the next job. 200 with a job, or 204 when the window closes
/// empty — the worker simply asks again.
pub async fn next(State(app): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    authorized(&app, &headers)?;
    // Recorded on arrival, not on success: a worker asking for work is alive
    // whether or not there is any, and that is precisely what the picker needs to
    // know before it promises anyone an answer.
    app.mark_worker_seen();

    let queued = app.job_queued();
    let deadline = Instant::now() + POLL_WINDOW;
    loop {
        // Arm the wake-up BEFORE looking, so a job queued while the query below
        // runs wakes us rather than being missed until the backstop. `notified()`
        // does not register until first polled, hence the explicit `enable()` —
        // without it the future would only start listening at the `await`, which
        // is exactly the window this is meant to close.
        let woken = queued.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();

        if let Some(job) = suggest_store::claim_next(&app.pool).await? {
            return Ok(Json(JobOut {
                id: job.id,
                prompt: job.prompt,
            })
            .into_response());
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
        let wait = RECHECK.min(deadline - now);
        // Whichever comes first: someone queued work, or it's time to look again.
        let _ = tokio::time::timeout(wait, woken).await;
    }
}

/// Take the model's answer, keep only feelings that are really in the vocabulary
/// this job was asked about, and cache them against the note they describe.
pub async fn result(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    Json(body): Json<ResultIn>,
) -> Result<StatusCode, AppError> {
    authorized(&app, &headers)?;
    app.mark_worker_seen();

    let Some(job) = suggest_store::job_for_completion(&app.pool, id).await? else {
        // The note changed and the job was replaced while the model was busy.
        // Nothing to record, and nothing wrong.
        return Ok(StatusCode::NO_CONTENT);
    };

    if let Some(err) = body.error.as_deref() {
        // Record the failure as "no suggestions for this wording" rather than
        // simply dropping the job. Dropping it would leave the note uncached, the
        // picker would queue it again on its next poll, and a model that chokes on
        // one note would be handed it every couple of seconds for as long as the
        // picker stayed open. Editing the note asks again; nothing else does.
        tracing::warn!("emotion worker failed job {id}: {err}");
        suggest_store::complete(&app.pool, id, &job, &[]).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    let raw = suggest::parse_tokens(body.content.as_deref().unwrap_or_default());
    let valid = job.candidates.iter().map(String::as_str).collect();
    // No `already` here: what is selected is a question for display time, and
    // baking today's selection into the cache would hide a suggestion the moment
    // it was deselected.
    let tokens = suggest::filter_suggestions(raw, &valid, &Default::default(), suggest::MAX_CACHED);
    suggest_store::complete(&app.pool, id, &job, &tokens).await?;
    Ok(StatusCode::NO_CONTENT)
}
