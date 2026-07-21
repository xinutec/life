//! Emotion suggestions: given a check-in note, ask Claude which feelings from the
//! app's own vocabulary best fit what was written, and return them ranked so the
//! picker can offer them first.
//!
//! The vocabulary (the candidate list) is supplied by the caller — it *is* the
//! frontend's feelings-wheel, so there is no second copy on the server to drift.
//! Claude only ranks; it never invents a word: every token it returns is validated
//! against the candidate set in [`filter_suggestions`], and anything that isn't in
//! the list is dropped before it can reach the picker.
//!
//! The network call is split from the parsing/validation so the latter is unit-
//! tested against captured JSON without reaching Anthropic.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;

/// Haiku: ~1s, a fraction of a cent, and ample for ranking against a fixed list.
const MODEL: &str = "claude-haiku-4-5-20251001";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The most suggestions to surface — a short, glanceable head of the list.
pub const MAX_SUGGESTIONS: usize = 6;

/// Request from the picker: the note, the whole feelings vocabulary as candidates,
/// and the tokens already chosen (never re-suggested).
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsRequest {
    /// The check-in note, as typed.
    pub note: String,
    /// Every feeling the picker can offer — token plus its gloss.
    pub candidates: Vec<EmotionCandidate>,
    /// Tokens already on the entry; excluded from the suggestions.
    #[serde(default)]
    pub already: Vec<String>,
}

/// One selectable feeling: its `Core/Name` token and the plain-English gloss.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EmotionCandidate {
    pub token: String,
    pub desc: String,
}

/// Response: suggested tokens, most-fitting first, each guaranteed to be one of
/// the request's candidates and not already chosen.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsResponse {
    pub suggestions: Vec<String>,
}

const SYSTEM_PROMPT: &str = "You help someone tag a wellbeing check-in with the feelings that match what they wrote. \
You are given the note and a fixed list of feelings, each as `token — meaning`. \
Choose the feelings from the list that are genuinely present in the note, ranked most-fitting first. \
Only choose feelings that are really there: returning few, or none, is correct — do not pad. \
Never invent a feeling; every token you return must be copied exactly from the list. \
Record your choices by calling the record_suggestions tool.";

/// The user turn: the note, then the candidate menu as `token — meaning` lines.
pub fn build_user_content(note: &str, candidates: &[EmotionCandidate]) -> String {
    let mut s = String::with_capacity(note.len() + candidates.len() * 48 + 64);
    s.push_str("Note:\n");
    s.push_str(note);
    s.push_str("\n\nFeelings to choose from (token — meaning):\n");
    for c in candidates {
        s.push_str(&c.token);
        s.push_str(" — ");
        s.push_str(&c.desc);
        s.push('\n');
    }
    s
}

/// Ask Claude to rank the candidates for this note. Returns the raw tokens it
/// chose, still *unvalidated* — the caller filters them against the vocabulary.
pub async fn request_suggestions(
    http: &reqwest::Client,
    api_key: &str,
    note: &str,
    candidates: &[EmotionCandidate],
) -> Result<Vec<String>> {
    let body = serde_json::json!({
        "model": MODEL,
        "max_tokens": 512,
        "system": SYSTEM_PROMPT,
        "messages": [{ "role": "user", "content": build_user_content(note, candidates) }],
        "tools": [{
            "name": "record_suggestions",
            "description": "Record the chosen feelings, ranked most-fitting first.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "tokens": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tokens copied exactly from the provided list, most-fitting first."
                    }
                },
                "required": ["tokens"]
            }
        }],
        "tool_choice": { "type": "tool", "name": "record_suggestions" }
    });

    let resp = http
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Anthropic request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic returned HTTP {status}: {text}");
    }
    let text = resp.text().await.context("Anthropic read failed")?;
    parse_tool_tokens(&text)
}

/// Pull the `tokens` array out of the forced `record_suggestions` tool call in an
/// Anthropic Messages response. Pure, so it is tested against captured JSON. A
/// response with no such tool call (e.g. the model returned text) yields none.
pub fn parse_tool_tokens(response_json: &str) -> Result<Vec<String>> {
    let v: serde_json::Value =
        serde_json::from_str(response_json).context("Anthropic response is not JSON")?;
    let content = v
        .get("content")
        .and_then(|c| c.as_array())
        .context("Anthropic response has no content array")?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
            && block.get("name").and_then(|n| n.as_str()) == Some("record_suggestions")
        {
            let tokens = block
                .get("input")
                .and_then(|i| i.get("tokens"))
                .and_then(|t| t.as_array())
                .context("record_suggestions call has no tokens array")?;
            return Ok(tokens
                .iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect());
        }
    }
    Ok(Vec::new())
}

/// Keep only real, not-already-chosen suggestions, de-duplicated in Claude's rank
/// order and capped at `max`. This is the guardrail that makes a hallucinated word
/// impossible: a token the vocabulary doesn't contain never survives.
pub fn filter_suggestions(
    raw: Vec<String>,
    valid: &HashSet<&str>,
    already: &HashSet<&str>,
    max: usize,
) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for tok in raw {
        if out.len() >= max {
            break;
        }
        if valid.contains(tok.as_str())
            && !already.contains(tok.as_str())
            && seen.insert(tok.clone())
        {
            out.push(tok);
        }
    }
    out
}
