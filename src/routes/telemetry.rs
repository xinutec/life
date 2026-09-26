//! Client activity trace: navigations and taps, POSTed in batches, logged into
//! the same stream as the request trace (so `kind=tap label="Find at Asda"`
//! sits before the `GET …/find/asda` it caused) and stored in `client_events`,
//! which outlives the pod's log. The client captures them centrally; see the
//! frontend's `telemetry.ts`.

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
/// `kind` and `path` are client-chosen too, so they are flattened like `label`,
/// and bounded because a value longer than its column fails the whole batch.
/// 16 leaves room for a kind a newer client sends.
const MAX_KIND: usize = 16;
/// Long enough for any route this app has, matching the column.
const MAX_PATH: usize = 512;

/// Format characters that are invisible or reorder display: zero-width
/// characters (U+200B, U+FEFF, the joiners) make a label that reads as empty,
/// and bidi overrides (U+202A–202E, U+2066–2069) make a log line display
/// something other than it says (Trojan Source). `char::is_control` covers only
/// Cc, and a named list avoids a Unicode tables crate.
fn is_deceptive_format(c: char) -> bool {
    matches!(c,
        '\u{00ad}'
        | '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}'
    )
}

/// Flatten a client-supplied label to a single harmless log field.
///
/// **The endpoint's security boundary**: a label containing a newline would
/// forge whole log lines.
///
/// Control characters become spaces, whitespace runs collapse, and the result is
/// capped in chars. `char::is_control` misses U+2028/U+2029; `split_whitespace`
/// catches them.
pub fn one_line(label: &str, max: usize) -> String {
    let unbroken: String = label
        .chars()
        .map(|c| {
            if c.is_control() || is_deceptive_format(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    unbroken
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// One event with every client-chosen field flattened and bounded — the only
/// shape allowed past this module, into the log or into the table.
///
/// A struct, so a field added to [`TelemetryEvent`] without sanitising it fails
/// to compile.
#[derive(Debug)]
pub struct Sanitised {
    pub kind: String,
    pub path: String,
    /// `""` when the event carries none — a nav. Empty rather than `Option`
    /// because the log field and the column both want a string either way.
    pub label: String,
}

/// Flatten and bound every field the client chooses.
pub fn sanitise(e: &TelemetryEvent) -> Sanitised {
    Sanitised {
        kind: one_line(&e.kind, MAX_KIND),
        path: one_line(&e.path, MAX_PATH),
        label: one_line(e.label.as_deref().unwrap_or_default(), MAX_LABEL),
    }
}

/// Append a batch to `client_events`.
///
/// One multi-row INSERT. Public for `tests/client_events_db.rs`: the endpoint
/// swallows write failures, so a broken INSERT is invisible from outside.
pub async fn store(
    pool: &sqlx::MySqlPool,
    user_id: &str,
    events: &[(Sanitised, i64)],
) -> sqlx::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let mut q = sqlx::QueryBuilder::new(
        "INSERT INTO client_events (user_id, kind, path, label, client_at_ms) ",
    );
    q.push_values(events, |mut row, (s, at)| {
        row.push_bind(user_id)
            .push_bind(&s.kind)
            .push_bind(&s.path)
            .push_bind(&s.label)
            .push_bind(at);
    });
    q.build().execute(pool).await.map(|_| ())
}

/// POST /api/telemetry — fold the client's events into the log stream. Always
/// 204: telemetry is best-effort, and the client neither reads the response nor
/// retries. Auth-gated so every line is attributed and the endpoint isn't an
/// open log-write for anyone.
pub async fn record(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(events): Json<Vec<TelemetryEvent>>,
) -> StatusCode {
    let rows: Vec<(Sanitised, i64)> = events
        .into_iter()
        .take(MAX_EVENTS)
        .map(|e| {
            let s = sanitise(&e);
            (s, e.at)
        })
        .collect();

    for (s, at) in &rows {
        tracing::info!(
            user = %user.user_id,
            kind = %s.kind,
            path = %s.path,
            label = %s.label,
            at,
            "client-event"
        );
    }

    // Best-effort: the client neither reads this nor retries, so a 500 saves
    // nothing. Logged at error, because a table that silently stopped filling
    // would read as "nobody used the app".
    if let Err(e) = store(&app.pool, &user.user_id, &rows).await {
        tracing::error!(
            user = %user.user_id,
            events = rows.len(),
            error = %e,
            "client-event store failed — these events are in the log only"
        );
    }
    StatusCode::NO_CONTENT
}
