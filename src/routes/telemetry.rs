//! Client activity trace: the navigations and taps the browser sees but the API
//! doesn't, POSTed in batches and folded into the SAME log stream as the
//! per-request trace. Read together they are one timeline —
//! `client-event kind=nav path=/product/56`, `client-event kind=tap
//! label="Find at Asda"`, then the `GET …/find/asda 200` the tap caused — so a
//! session reconstructs without any per-screen instrumentation. The client
//! captures it all from two central points (Router events + one global click
//! listener); see frontend `telemetry.ts`.
//!
//! Events are BOTH logged and stored (`client_events`, migration 0041). The log
//! line is what makes one session readable — interleaved with the per-request
//! trace, it is a timeline. The table is what makes a *month* readable, which the
//! log cannot be: it was one pod's buffer, 28 hours deep, erased on restart. The
//! two answer different questions, so this keeps both.

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
/// `kind` and `path` are chosen by the client too, and both were written into the
/// log line raw — so the forgery [`one_line`] exists to stop was reachable through
/// either of them, not only through `label`. They are bounded as well as
/// flattened because they are now stored, and a value longer than its column
/// would fail the insert for the whole batch.
///
/// The real vocabulary is "nav" and "tap"; 16 leaves room for a kind a newer
/// client sends that this server has not heard of.
const MAX_KIND: usize = 16;
/// Long enough for any route this app has, matching the column.
const MAX_PATH: usize = 512;

/// Format characters that are invisible, or that reorder what is displayed.
///
/// `char::is_control` covers categories Cc and nothing else, and Rust's std has
/// no Unicode category table — so these are named explicitly. Two reasons they
/// matter here, and the second is the sharper one:
///
/// - **Zero-width characters** (U+200B, U+FEFF, the word joiners) are invisible,
///   so a label made of them reads as empty while occupying the whole cap.
/// - **Bidi overrides** (U+202A–202E, U+2066–2069) reorder the *rendering* of
///   the text around them. A log line containing one can be made to display
///   something other than what it says — the Trojan Source trick, pointed at the
///   record rather than at source code.
///
/// A deny-list of what can deceive rather than all of category Cf, because
/// pulling a Unicode tables crate in for this would be disproportionate. Stated
/// so the limit is known rather than assumed.
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
/// **This is the security boundary of the endpoint, not tidiness.** A label is
/// verbatim UI text and it is written into a log line as `label=…`. A label
/// containing a newline therefore forges *whole log lines* — including further
/// `client-event` lines attributed to someone else, or lines that look like they
/// came from another component entirely. The log stops being evidence, which is
/// the one thing it exists to be.
///
/// Control characters become spaces, runs of whitespace collapse, and the result
/// is capped. `char::is_control` covers C0 and C1 but *not* U+2028 and U+2029,
/// which end a line in some renderers; `split_whitespace` catches those, so the
/// two passes together cover both. Capped in `chars` rather than bytes so a
/// multi-byte glyph is never split down the middle.
///
/// Public so `tests/telemetry.rs` can exercise it directly: it is the one part
/// of this endpoint an attacker chooses the input to.
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
/// A struct rather than three loose locals so that adding a field to
/// [`TelemetryEvent`] and forgetting to sanitise it is a compile error here
/// rather than a silent hole: that is exactly how `kind` and `path` came to be
/// written raw while `label` was carefully guarded.
#[derive(Debug)]
pub struct Sanitised {
    pub kind: String,
    pub path: String,
    /// `""` when the event carries none — a nav. Empty rather than `Option`
    /// because the log field and the column both want a string either way.
    pub label: String,
}

/// Flatten and bound every field the client chooses.
///
/// Public so `tests/telemetry.rs` can exercise it: these are the values an
/// attacker picks, and the endpoint's whole security boundary is that they
/// cannot forge a log line.
pub fn sanitise(e: &TelemetryEvent) -> Sanitised {
    Sanitised {
        kind: one_line(&e.kind, MAX_KIND),
        path: one_line(&e.path, MAX_PATH),
        label: one_line(e.label.as_deref().unwrap_or_default(), MAX_LABEL),
    }
}

/// Append a batch to `client_events`.
///
/// One multi-row INSERT, not a statement per event: a batch is a handful of rows
/// and the round trips dominate. Returns the error rather than handling it — the
/// caller decides what a failed write costs, and that decision belongs at the
/// endpoint where the best-effort contract is stated.
///
/// Public so `tests/client_events_db.rs` can run it against a real MariaDB. The
/// query is a runtime string, so executing it IS the check on it — and the
/// endpoint swallows write failures by design, which would make a broken INSERT
/// invisible from the outside.
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

    // Best-effort is the contract, and it is a deliberate choice rather than a
    // swallowed error: the client neither reads this response nor retries, so
    // answering 500 would cost the app a visible failure and still not save the
    // events. What must not happen is failing SILENTLY — a table that quietly
    // stopped filling would read as "nobody used the app", which is the exact
    // conclusion this data exists to support or refute. So the write is logged at
    // error level, and the events survive in the log line above either way.
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
