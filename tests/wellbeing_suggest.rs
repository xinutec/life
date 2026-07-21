//! Emotion-suggestion parsing + validation, exercised without a model. What the
//! worker posts back is the model's raw text, so these fixtures are the shapes a
//! small local model actually replies with — the JSON it was asked for, the same
//! JSON in a code fence, and prose when it decided not to cooperate.

use std::collections::HashSet;

use life::wellbeing::suggest::{filter_suggestions, note_hash, parse_tokens};

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
