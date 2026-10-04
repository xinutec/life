//! Emotion-suggestion parsing + validation, exercised without a model. What the
//! worker posts back is the model's raw text, so these fixtures are the shapes a
//! small local model actually replies with — the JSON it was asked for, the same
//! JSON in a code fence, and prose when it decided not to cooperate.

use std::collections::HashSet;

use life::wellbeing::suggest::{self, filter_suggestions, note_hash, parse_tokens};

#[test]
fn parses_the_ranked_tokens_it_was_asked_for() {
    let tokens = parse_tokens(r#"{"tokens": ["Sad/Low", "Sad/Empty", "Happy/Calm"]}"#);
    assert_eq!(tokens, vec!["Sad/Low", "Sad/Empty", "Happy/Calm"]);
}

#[test]
fn parses_tokens_wrapped_in_a_code_fence() {
    // Instruction-tuned models fence JSON by reflex, however plainly you ask them
    // not to. Unwrapping it is cheaper than losing the answer.
    let fenced = "```json\n{\"tokens\": [\"Sad/Low\"]}\n```";
    assert_eq!(parse_tokens(fenced), vec!["Sad/Low"]);
}

#[test]
fn prose_instead_of_json_yields_no_tokens() {
    // The model replied in prose: no suggestions, and nothing to report — "I
    // couldn't find any" is a legitimate answer, not a failure.
    assert_eq!(
        parse_tokens("I'm not sure which feelings fit."),
        Vec::<String>::new()
    );
}

#[test]
fn json_without_a_tokens_field_yields_no_tokens() {
    assert_eq!(
        parse_tokens(r#"{"feelings": ["Sad/Low"]}"#),
        Vec::<String>::new()
    );
}

#[test]
fn the_note_hash_ignores_surrounding_whitespace() {
    // Reopening the picker after a stray trailing space must read as the same
    // wording, or it would throw away a perfectly good cached answer.
    assert_eq!(note_hash("  a hard morning\n"), note_hash("a hard morning"));
    assert_ne!(note_hash("a hard morning"), note_hash("a hard afternoon"));
}

fn set<'a>(items: &'a [&'a str]) -> HashSet<&'a str> {
    items.iter().copied().collect()
}

#[test]
fn drops_hallucinated_tokens_not_in_the_vocabulary() {
    let valid = set(&["Sad/Low", "Happy/Calm"]);
    let already = HashSet::new();
    let raw = vec!["Sad/Low".into(), "Sad/Made-up".into(), "Happy/Calm".into()];
    // "Sad/Made-up" is not a candidate → it never survives.
    assert_eq!(
        filter_suggestions(raw, &valid, &already, 6),
        vec!["Sad/Low", "Happy/Calm"]
    );
}

#[test]
fn excludes_already_chosen_tokens() {
    let valid = set(&["Sad/Low", "Happy/Calm", "Sad/Empty"]);
    let already = set(&["Sad/Empty"]);
    let raw = vec!["Sad/Empty".into(), "Sad/Low".into()];
    assert_eq!(
        filter_suggestions(raw, &valid, &already, 6),
        vec!["Sad/Low"]
    );
}

#[test]
fn dedups_in_rank_order_and_caps_at_max() {
    let valid = set(&["A/One", "B/Two", "C/Three"]);
    let already = HashSet::new();
    let raw = vec![
        "A/One".into(),
        "A/One".into(), // duplicate → kept once, in first position
        "B/Two".into(),
        "C/Three".into(),
    ];
    assert_eq!(
        filter_suggestions(raw, &valid, &already, 2),
        vec!["A/One", "B/Two"]
    );
}

// --- the rollover preload's schedule -----------------------------------------
//
// The day's few-shot is cut at end-of-yesterday, so the whole prompt changes at
// the UTC rollover and its KV-cache prefix has to be rebuilt. A timer does it
// with nobody waiting; this is the arithmetic that decides when, kept pure so the
// boundary — the part most likely to be wrong — is testable at all.

use chrono::{TimeZone, Utc};
use life::wellbeing::suggest::{ROLLOVER_WARM_AFTER_MIDNIGHT, until_rollover_warm};

#[test]
fn just_before_the_window_waits_only_the_remainder() {
    let now = Utc.with_ymd_and_hms(2026, 7, 25, 0, 4, 0).unwrap();
    assert_eq!(until_rollover_warm(now).as_secs(), 60);
}

#[test]
fn mid_morning_waits_until_tomorrow() {
    let now = Utc.with_ymd_and_hms(2026, 7, 25, 11, 5, 0).unwrap();
    // 12h55m to midnight, plus the 5m offset.
    assert_eq!(
        until_rollover_warm(now).as_secs(),
        12 * 3600 + 55 * 60 + 300
    );
}

#[test]
fn landing_exactly_on_the_target_schedules_the_next_day_not_a_busy_loop() {
    // The failure this guards is a zero-length sleep re-firing forever, which
    // would hammer the worker with preloads instead of doing one a night.
    let now = Utc.with_ymd_and_hms(2026, 7, 25, 0, 5, 0).unwrap();
    assert_eq!(until_rollover_warm(now).as_secs(), 24 * 3600);
}

#[test]
fn the_wait_is_always_positive_and_never_more_than_a_day() {
    // Whatever the clock says, the timer must fire within one day and never
    // instantly: both ends are how a scheduler stops being one.
    for hour in 0..24 {
        for minute in [0, 4, 5, 6, 30, 59] {
            let now = Utc.with_ymd_and_hms(2026, 7, 25, hour, minute, 0).unwrap();
            let wait = until_rollover_warm(now);
            assert!(wait.as_secs() > 0, "{hour}:{minute} scheduled instantly");
            assert!(
                wait.as_secs() <= 24 * 3600,
                "{hour}:{minute} waits over a day"
            );
        }
    }
}

#[test]
fn it_crosses_a_month_end() {
    let now = Utc.with_ymd_and_hms(2026, 7, 31, 23, 0, 0).unwrap();
    assert_eq!(
        until_rollover_warm(now).as_secs(),
        3600 + ROLLOVER_WARM_AFTER_MIDNIGHT.as_secs()
    );
}

// --- respond: what the picker is told ---

fn cands(tokens: &[&str]) -> Vec<suggest::EmotionCandidate> {
    tokens
        .iter()
        .map(|t| suggest::EmotionCandidate {
            token: (*t).into(),
            desc: String::new(),
        })
        .collect()
}

fn waiting(secs: i64, being_worked: bool, worker_alive: bool) -> suggest::Progress {
    suggest::Progress::Waiting {
        secs,
        being_worked,
        worker_alive,
    }
}

#[test]
fn an_answer_for_this_wording_is_neither_stale_nor_pending() {
    let r = suggest::respond(
        vec!["Sad/Low".into(), "Sad/Empty".into()],
        suggest::Progress::Answered,
        &cands(&["Sad/Low", "Sad/Empty"]),
        &["Sad/Empty".into()],
    );
    assert_eq!(
        r.suggestions,
        ["Sad/Low"],
        "what is already chosen is not offered"
    );
    assert!(!r.stale && !r.pending && r.thinking_secs.is_none());
}

#[test]
fn an_earlier_wordings_answer_is_shown_as_stale_while_a_new_one_is_worked_on() {
    let r = suggest::respond(
        vec!["Sad/Low".into()],
        waiting(12, true, false),
        &cands(&["Sad/Low"]),
        &[],
    );
    assert!(r.stale);
    assert!(
        r.pending,
        "a claim proves a worker even when it has not polled"
    );
    assert_eq!(r.thinking_secs, Some(12));
}

#[test]
fn nothing_is_promised_without_a_worker() {
    let r = suggest::respond(vec![], waiting(40, false, false), &cands(&["Sad/Low"]), &[]);
    assert!(!r.pending && r.thinking_secs.is_none());
    assert!(!r.stale, "no suggestions are not stale ones");
    let r = suggest::respond(vec![], waiting(3, false, true), &cands(&["Sad/Low"]), &[]);
    assert!(r.pending, "a worker that polled recently will answer");
    assert_eq!(r.thinking_secs, Some(3));
}
