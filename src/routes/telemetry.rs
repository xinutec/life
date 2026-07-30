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
        let label = one_line(&e.label.unwrap_or_default(), MAX_LABEL);
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
