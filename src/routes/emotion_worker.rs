//! What the emotion-suggestion worker on the Mac polls; the fleet cannot dial the
//! Mac. Auth is a bearer token: the worker acts for no user, and its jobs carry no
//! identity.

use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::state::AppState;
use crate::wellbeing::suggest;
use crate::wellbeing::suggest_store;

/// Inside any proxy's idle timeout.
const POLL_WINDOW: Duration = Duration::from_secs(25);
/// A backstop for jobs this process's signal cannot see (queued by another pod).
const RECHECK: Duration = Duration::from_secs(5);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobOut {
    id: u64,
    prompt: serde_json::Value,
    /// Load the model and warm its cache, posting no result; `id` is 0.
    #[serde(default)]
    warm: bool,
}

/// The model's raw text: every judgement about it belongs here, where the
/// vocabulary is known.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultIn {
    #[serde(default)]
    content: Option<String>,
    /// Instead of `content`, when the model could not run.
    #[serde(default)]
    error: Option<String>,
}

/// Missing, wrong and unconfigured all answer the same 401.
fn authorized(app: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(expected) = app.cfg.emotion_worker_token.as_deref() else {
        return Err(AppError::Unauthorized);
    };
    let given = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    // Constant time: the refusal's timing says nothing.
    if given.is_empty() || !bool::from(given.as_bytes().ct_eq(expected.as_bytes())) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

/// 200 with a job, or 204 when the window closes empty.
pub async fn next(State(app): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    authorized(&app, &headers)?;
    // On arrival: asking is what shows the worker is alive.
    app.mark_worker_seen();

    // During shutdown, answer empty and let the worker find the next pod.
    let mut stopping = app.stopping();
    if *stopping.borrow() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    let queued = app.job_queued();
    let deadline = Instant::now() + POLL_WINDOW;
    loop {
        // Armed before looking, or a job queued meanwhile waits for the backstop.
        // `notified()` only registers once polled, hence `enable()`.
        let woken = queued.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();

        if let Some(job) = suggest_store::claim_next(&app.pool).await? {
            return Ok(Json(JobOut {
                id: job.id,
                prompt: job.prompt,
                warm: false,
            })
            .into_response());
        }
        // An empty queue and a note being written: warm the model.
        if let Some(system) = app.take_warm() {
            return Ok(Json(JobOut {
                id: 0,
                prompt: serde_json::json!({ "system": system, "user": "" }),
                warm: true,
            })
            .into_response());
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
        let wait = RECHECK.min(deadline - now);
        tokio::select! {
            _ = tokio::time::timeout(wait, woken) => {}
            _ = stopping.wait_for(|s| *s) => return Ok(StatusCode::NO_CONTENT.into_response()),
        }
    }
}

/// Keep the feelings really in this job's vocabulary, cached against its note.
pub async fn result(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    Json(body): Json<ResultIn>,
) -> Result<StatusCode, AppError> {
    authorized(&app, &headers)?;
    app.mark_worker_seen();

    let Some(job) = suggest_store::job_for_completion(&app.pool, id).await? else {
        // The note changed while the model was busy; nothing to record.
        return Ok(StatusCode::NO_CONTENT);
    };

    if let Some(err) = body.error.as_deref() {
        // Recorded as "none" rather than dropped: a dropped job is re-queued on
        // the picker's next poll, every couple of seconds.
        tracing::warn!("emotion worker failed job {id}: {err}");
        suggest_store::complete(&app.pool, id, &job, &[]).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    let raw = suggest::parse_tokens(body.content.as_deref().unwrap_or_default());
    let valid = job.candidates.iter().map(String::as_str).collect();
    // No `already`: the selection is for display time, not the cache.
    let tokens = suggest::filter_suggestions(raw, &valid, &Default::default(), suggest::MAX_CACHED);
    suggest_store::complete(&app.pool, id, &job, &tokens).await?;
    Ok(StatusCode::NO_CONTENT)
}
