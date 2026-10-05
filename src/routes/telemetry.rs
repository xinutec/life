//! The client's navigations and taps, logged into the request trace (so a tap
//! sits before the request it caused) and kept in `client_events`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use ts_rs::TS;

use crate::session::AuthUser;
use crate::state::AppState;

/// `kind` is "nav" or "tap"; a tap's `label` is the control's visible text.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TelemetryEvent {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Client epoch millis: a batch lands at once, so only these order it.
    #[ts(type = "number")]
    pub at: i64,
}

/// So one POST cannot flood the log.
const MAX_EVENTS: usize = 100;
/// In chars, so a glyph is never split.
const MAX_LABEL: usize = 160;
/// Client-chosen too, so flattened and bounded to its column; room for a newer kind.
const MAX_KIND: usize = 16;
const MAX_PATH: usize = 512;

/// Invisible and reordering characters: zero-width ones make a label read as
/// empty, bidi overrides make a log line say something else (Trojan Source).
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

/// The endpoint's security boundary: one harmless log field, as a newline would
/// forge log lines. `split_whitespace` also catches U+2028/2029.
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

/// The only shape allowed past this module. A struct, so an unsanitised new
/// field cannot compile.
#[derive(Debug)]
pub struct Sanitised {
    pub kind: String,
    pub path: String,
    /// `""` for a nav.
    pub label: String,
}

pub fn sanitise(e: &TelemetryEvent) -> Sanitised {
    Sanitised {
        kind: one_line(&e.kind, MAX_KIND),
        path: one_line(&e.path, MAX_PATH),
        label: one_line(e.label.as_deref().unwrap_or_default(), MAX_LABEL),
    }
}

/// Public for its test: the endpoint swallows write failures.
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

/// POST /api/telemetry → always 204: the client neither reads it nor retries.
/// Auth-gated, so the log is not open to anyone.
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

    // Logged at error: an empty table would read as "nobody used the app".
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
