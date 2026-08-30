//! The telemetry label is the endpoint's security boundary, not a cosmetic cap.
//!
//! A label is verbatim UI text and it is written into a log line as `label=…`.
//! A newline inside it therefore forges *whole log lines* — including further
//! `client-event` lines attributed to someone else, or lines that look like they
//! came from another component. The log stops being the evidence it exists to be.

use life::routes::telemetry::one_line;

/// Cap used throughout, matching the endpoint's own.
const MAX: usize = 160;

#[test]
fn a_label_cannot_forge_a_log_line() {
    let forged = "ok\nclient-event kind=tap path=/admin label=Delete everything";
    let flat = one_line(forged, MAX);
    assert!(
        !flat.contains('\n'),
        "a newline survived into the log: {flat:?}"
    );
    assert!(!flat.contains('\r'));
    assert_eq!(
        flat,
        "ok client-event kind=tap path=/admin label=Delete everything"
    );
}

#[test]
fn the_separators_is_control_misses_are_still_flattened() {
    // U+2028 and U+2029 are Zl/Zp, not Cc, so `char::is_control` says nothing
    // about them — and some renderers break a line on both.
    assert_eq!(
        one_line("before\u{2028}after\u{2029}end", MAX),
        "before after end"
    );
}

#[test]
fn a_zero_width_character_cannot_hide_inside_a_label() {
    // U+200B is Cf: not whitespace, not a control character, and invisible. A
    // label made of them would read as empty while occupying the whole cap.
    assert_eq!(one_line("a\u{200b}b", MAX), "a b");
}

#[test]
fn an_ordinary_label_is_left_alone() {
    assert_eq!(one_line("Render as music", MAX), "Render as music");
}

#[test]
fn a_long_label_is_capped_without_splitting_a_glyph() {
    // Counted in chars, not bytes: truncating "é" mid-sequence would emit
    // invalid text into the log.
    let flat = one_line(&"é".repeat(500), MAX);
    assert_eq!(flat.chars().count(), MAX);
}

#[test]
fn a_bidi_override_cannot_disguise_what_the_line_says() {
    // The sharper half of the format-character problem. U+202E flips the
    // rendering of everything after it, so a label can be made to *display* as
    // something other than its content — Trojan Source, aimed at the record
    // instead of at source code. Invisible, so nobody reviewing the log sees why.
    let flat = one_line("Save\u{202e}\u{202d}Delete", MAX);
    assert!(
        !flat.contains('\u{202e}'),
        "a bidi override survived: {flat:?}"
    );
    assert_eq!(flat, "Save Delete");
}

// ---------------------------------------------------------------------------
// `label` was the only field flattened, but it is not the only one the client
// chooses. `kind` and `path` are deserialised straight off the wire and were
// written into the log line verbatim — so the forgery this file exists to
// prevent was reachable through either of them.

use life::routes::telemetry::{TelemetryEvent, sanitise};

fn event(kind: &str, path: &str, label: Option<&str>) -> TelemetryEvent {
    TelemetryEvent {
        kind: kind.into(),
        path: path.into(),
        label: label.map(Into::into),
        at: 1_756_000_000_000,
    }
}

#[test]
fn a_path_cannot_forge_a_log_line() {
    let e = event(
        "nav",
        "/ok\nclient-event kind=tap path=/admin label=Delete everything",
        None,
    );
    let s = sanitise(&e);
    assert!(
        !s.path.contains('\n'),
        "a newline survived in path: {:?}",
        s.path
    );
    assert_eq!(
        s.path,
        "/ok client-event kind=tap path=/admin label=Delete everything"
    );
}

#[test]
fn a_kind_cannot_forge_a_log_line() {
    let s = sanitise(&event("nav\nclient-event kind=tap", "/x", None));
    assert!(
        !s.kind.contains('\n'),
        "a newline survived in kind: {:?}",
        s.kind
    );
}

#[test]
fn every_client_chosen_field_is_bounded() {
    // Unbounded, one event could fill the log — and, once stored, overflow the
    // column and fail the whole batch's insert.
    let s = sanitise(&event(
        &"k".repeat(9_000),
        &"/p".repeat(9_000),
        Some(&"l".repeat(9_000)),
    ));
    assert!(
        s.kind.chars().count() <= 16,
        "kind unbounded: {}",
        s.kind.chars().count()
    );
    assert!(
        s.path.chars().count() <= 512,
        "path unbounded: {}",
        s.path.chars().count()
    );
    assert!(
        s.label.chars().count() <= 160,
        "label unbounded: {}",
        s.label.chars().count()
    );
}

#[test]
fn an_absent_label_stays_empty_rather_than_becoming_a_word() {
    assert_eq!(sanitise(&event("nav", "/wellbeing", None)).label, "");
}
