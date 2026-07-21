//! Emotion-suggestion parsing + validation, exercised without a model. The
//! fixtures are the shape of a real OpenAI-compatible chat-completions response
//! (mlx_lm.server / Ollama `/v1`).

use std::collections::HashSet;

use life::wellbeing::suggest::{filter_suggestions, parse_chat_tokens};

/// The ranked list arrives as JSON inside `choices[0].message.content`.
const CHAT_RESPONSE: &str = r#"{
  "id": "chatcmpl-1",
  "object": "chat.completion",
  "model": "mlx-community/Qwen2.5-7B-Instruct-4bit",
  "choices": [
    { "index": 0, "message": { "role": "assistant", "content": "{\"tokens\": [\"Sad/Low\", \"Sad/Empty\", \"Happy/Calm\"]}" } }
  ]
}"#;

#[test]
fn parses_tokens_from_message_content() {
    let tokens = parse_chat_tokens(CHAT_RESPONSE).expect("parse");
    assert_eq!(tokens, vec!["Sad/Low", "Sad/Empty", "Happy/Calm"]);
}

#[test]
fn content_that_isnt_the_expected_json_yields_no_tokens() {
    // The model replied in prose instead of the JSON we asked for: no suggestions,
    // not an error.
    let prose =
        r#"{ "choices": [ { "message": { "content": "I'm not sure which feelings fit." } } ] }"#;
    assert_eq!(
        parse_chat_tokens(prose).expect("parse"),
        Vec::<String>::new()
    );
}

#[test]
fn malformed_json_is_an_error_not_a_silent_empty() {
    assert!(parse_chat_tokens("not json").is_err());
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
