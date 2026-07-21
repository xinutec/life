//! Emotion-suggestion parsing + validation, exercised without the network. The
//! `tokens` fixture is the shape of a real Anthropic Messages response to a forced
//! `record_suggestions` tool call.

use std::collections::HashSet;

use life::wellbeing::suggest::{filter_suggestions, parse_tool_tokens};

/// A forced-tool-call response: `content` carries a single `tool_use` block whose
/// `input.tokens` is the ranked list. (Real responses also prefix an optional text
/// block; include one to prove we scan past it.)
const TOOL_RESPONSE: &str = r#"{
  "id": "msg_01",
  "type": "message",
  "role": "assistant",
  "content": [
    { "type": "text", "text": "Here are the fitting feelings." },
    {
      "type": "tool_use",
      "id": "toolu_01",
      "name": "record_suggestions",
      "input": { "tokens": ["Sad/Low", "Neutral/Flat", "Happy/Calm"] }
    }
  ],
  "stop_reason": "tool_use"
}"#;

#[test]
fn parses_tokens_from_the_tool_call() {
    let tokens = parse_tool_tokens(TOOL_RESPONSE).expect("parse");
    assert_eq!(tokens, vec!["Sad/Low", "Neutral/Flat", "Happy/Calm"]);
}

#[test]
fn a_text_only_response_yields_no_tokens() {
    // The model answered in prose instead of calling the tool: no suggestions,
    // not an error.
    let text_only = r#"{ "content": [ { "type": "text", "text": "I'm not sure." } ] }"#;
    assert_eq!(
        parse_tool_tokens(text_only).expect("parse"),
        Vec::<String>::new()
    );
}

#[test]
fn malformed_json_is_an_error_not_a_silent_empty() {
    assert!(parse_tool_tokens("not json").is_err());
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
