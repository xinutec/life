//! Client activity trace: the navigations and taps the browser sees but the API
//! doesn't, POSTed in batches and folded into the SAME log stream as the
//! per-request trace. Read together they are one timeline —
//! `client-event kind=nav path=/product/56`, `client-event kind=tap
//! label="Find at Asda"`, then the `GET …/find/asda 200` the tap caused — so a
//! session reconstructs without any per-screen instrumentation. The client
//! captures it all from two central points (Router events + one global click
//! listener); see frontend `telemetry.ts`.
//!
//! There is NO storage here: these are logs, not data. The endpoint exists only
//! to move the client's events into the backend log where they can be read, then
//! forgets them.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use ts_rs::TS;

use crate::session::AuthUser;
use crate::state::AppState;

/// One thing that happened in the client. `kind` is "nav" (a route change,
/// `label` absent) or "tap" (a control, `label` its visible text, verbatim).
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TelemetryEvent {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Client clock, epoch millis. Kept because a batch lands all at once, so the
    /// server receive time can't order events within it; the client's can.
    #[ts(type = "number")]
    pub at: i64,
}

/// A per-batch cap so a buggy or hostile client can't turn one POST into a log
/// flood — the real client batches a handful at a time.
const MAX_EVENTS: usize = 100;
/// Labels are verbatim UI text; bound them so a pathological one can't bloat a
/// log line. Counted in chars, not bytes, to never split a multi-byte glyph.
const MAX_LABEL: usize = 160;

/// POST /api/telemetry — fold the client's events into the log stream. Always
/// 204: telemetry is best-effort, and the client neither reads the response nor
/// retries. Auth-gated so every line is attributed and the endpoint isn't an
/// open log-write for anyone.
pub async fn record(
    State(_app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(events): Json<Vec<TelemetryEvent>>,
) -> StatusCode {
    for e in events.into_iter().take(MAX_EVENTS) {
        let label: String = e
            .label
            .unwrap_or_default()
            .chars()
            .take(MAX_LABEL)
            .collect();
        tracing::info!(
            user = %user.user_id,
            kind = %e.kind,
            path = %e.path,
            label = %label,
            at = e.at,
            "client-event"
        );
    }
    StatusCode::NO_CONTENT
}
