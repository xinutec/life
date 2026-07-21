//! Emotion suggestions: given a check-in note, ask a local model which feelings
//! from the app's own vocabulary best fit what was written, and return them ranked
//! so the picker can offer them first.
//!
//! The model is a small, self-hosted, Ollama-compatible server (e.g. on the Mac) —
//! nothing leaves your hardware. It's fed two things: the vocabulary (the
//! candidate list, sent by the picker so there's no second copy to drift) and a
//! **few-shot of your own past taggings**, which teaches it your personal
//! calibration — that you reach for *Low* not *Grief*, *Flat* for a neutral day,
//! and so on. In an offline eval on held-out check-ins, that personalisation
//! roughly doubled agreement with what you actually picked.
//!
//! Every token the model returns is validated against the candidate set in
//! [`filter_suggestions`]; anything not in the list is dropped, so a hallucinated
//! feeling can never reach the picker. The whole thing is best-effort: a slow,
//! down, or unset server just yields no suggestions (see the route handler).
//!
//! The network call is split from the prompt-building and parsing so the latter
//! are unit-tested without a model.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::collections::HashSet;
use std::time::Duration;
use ts_rs::TS;

/// Most recent past taggings to few-shot. Enough to teach a style; bounded so the
/// prompt stays a fixed, cacheable size as history grows.
pub const MAX_EXAMPLES: u32 = 80;
/// The most suggestions to surface — a short, glanceable head of the list.
pub const MAX_SUGGESTIONS: usize = 6;
/// Hard cap on the whole call: a reachable model answers in ~1-3s; anything longer
/// (a slow or half-up server) is abandoned so the picker's spinner never hangs.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Request from the picker: the note, the whole feelings vocabulary as candidates,
/// and the tokens already chosen.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsRequest {
    pub note: String,
    pub candidates: Vec<EmotionCandidate>,
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

/// One past tagging, for the few-shot: what was written, and what was chosen.
pub struct EmotionExample {
    pub note: String,
    pub tokens: Vec<String>,
}

const SYSTEM_INSTRUCTIONS: &str = "You help someone tag a wellbeing check-in with the feelings that match what they wrote. \
You are given the note and a fixed list of feelings, each as `token — meaning`. \
Choose the feelings from the list that are genuinely present in the note, ranked most-fitting first. \
Only choose feelings that are really there: returning few, or none, is correct — do not pad. \
Never invent a feeling; every token you return must be copied exactly from the list.";

/// The system turn: instructions, the candidate menu, then (if any) the user's own
/// past taggings as few-shot. The whole thing is fixed between check-ins, so an
/// Ollama-compatible server can cache its KV prefix and only re-read the note.
pub fn build_system(candidates: &[EmotionCandidate], examples: &[EmotionExample]) -> String {
    let mut s = String::from(SYSTEM_INSTRUCTIONS);
    s.push_str("\n\nFeelings to choose from (token — meaning):\n");
    for c in candidates {
        s.push_str(&c.token);
        s.push_str(" — ");
        s.push_str(&c.desc);
        s.push('\n');
    }
    if !examples.is_empty() {
        s.push_str(
            "\nHere is how THIS person has tagged their own past notes — learn their personal style \
             and which words they reach for. You may still suggest a fitting feeling they have not \
             used before.\n",
        );
        for e in examples {
            s.push_str("\nNote: ");
            s.push_str(&e.note.replace('\n', " "));
            s.push_str("\nFeelings: ");
            s.push_str(&serde_json::to_string(&e.tokens).unwrap_or_else(|_| "[]".into()));
            s.push('\n');
        }
    }
    s
}

fn build_user(note: &str) -> String {
    format!(
        "Note:\n{note}\n\nReturn JSON {{\"tokens\": [up to {MAX_SUGGESTIONS} tokens copied exactly \
         from the list, most-fitting first]}}."
    )
}

/// Fetch the user's own labelled check-ins (note + chosen feelings) for the
/// few-shot, most recent first.
pub async fn fetch_examples(
    pool: &MySqlPool,
    user_id: &str,
    limit: u32,
) -> sqlx::Result<Vec<EmotionExample>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT note, emotions FROM wellbeing \
         WHERE user_id = ? AND deleted_at IS NULL \
           AND emotions IS NOT NULL AND emotions <> '[]' AND emotions <> '' \
           AND note IS NOT NULL AND note <> '' \
         ORDER BY recorded_at DESC LIMIT ?",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(note, emotions)| {
            let tokens: Vec<String> = serde_json::from_str(&emotions).ok()?;
            (!tokens.is_empty()).then_some(EmotionExample { note, tokens })
        })
        .collect())
}

/// Ask the model to rank the candidates for this note. Returns the raw tokens it
/// chose, still *unvalidated* — the caller filters them against the vocabulary.
pub async fn request_suggestions(
    http: &reqwest::Client,
    base_url: &str,
    model: &str,
    note: &str,
    candidates: &[EmotionCandidate],
    examples: &[EmotionExample],
) -> Result<Vec<String>> {
    let body = serde_json::json!({
        "model": model,
        "temperature": 0,
        "max_tokens": 128,
        "messages": [
            { "role": "system", "content": build_system(candidates, examples) },
            { "role": "user", "content": build_user(note) },
        ],
    });

    // OpenAI-compatible chat completions — spoken by mlx_lm.server (the Mac,
    // reusing the Qwen the recall stack already runs) and by Ollama's /v1 alike.
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .timeout(TIMEOUT)
        .json(&body)
        .send()
        .await
        .context("emotion model request failed")?;
    if !resp.status().is_success() {
        anyhow::bail!("emotion model returned HTTP {}", resp.status());
    }
    let text = resp.text().await.context("emotion model read failed")?;
    parse_chat_tokens(&text)
}

/// Pull the ranked tokens out of an OpenAI-compatible chat-completions response,
/// whose `choices[0].message.content` is itself the JSON `{"tokens": [...]}` we
/// asked for. Pure, so it is tested against captured JSON. Content that isn't the
/// expected shape yields no tokens rather than erroring — the model simply gave us
/// nothing usable.
pub fn parse_chat_tokens(response_json: &str) -> Result<Vec<String>> {
    let v: serde_json::Value =
        serde_json::from_str(response_json).context("emotion model response is not JSON")?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .context("emotion model response has no choices[0].message.content")?;
    let Ok(inner) = serde_json::from_str::<serde_json::Value>(content) else {
        return Ok(Vec::new());
    };
    Ok(inner
        .get("tokens")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default())
}

/// Keep only real, not-already-chosen suggestions, de-duplicated in the model's
/// rank order and capped at `max`. This is the guardrail that makes a hallucinated
/// word impossible: a token the vocabulary doesn't contain never survives.
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
