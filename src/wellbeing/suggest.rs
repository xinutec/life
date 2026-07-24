//! Emotion suggestions: given a check-in note, ask a local model which feelings
//! from the app's own vocabulary best fit what was written, and return them ranked
//! so the picker can offer them first.
//!
//! The model runs on the Mac and nothing leaves your hardware — but the Mac is a
//! one-way WireGuard peer, so the fleet may not dial it. Generation therefore
//! happens through a queue the Mac *polls*: this module builds a self-contained
//! prompt, [`store`](super::suggest_store) parks it as a job, and the worker posts
//! the model's answer back. The pod only ever accepts connections.
//!
//! The prompt is fed two things: the vocabulary (the candidate list, sent by the
//! picker so there's no second copy to drift) and a **few-shot of your own past
//! taggings**, which teaches it your personal calibration — that you reach for
//! *Low* not *Grief*, *Flat* for a neutral day, and so on. In an offline eval on
//! held-out check-ins, that personalisation roughly doubled agreement with what
//! you actually picked.
//!
//! Every token the model returns is validated against the candidate set in
//! [`filter_suggestions`]; anything not in the list is dropped, so a hallucinated
//! feeling can never reach the picker. The whole thing is best-effort: with no
//! worker running, the picker simply shows the plain wheel.
//!
//! Prompt-building and parsing are pure, so they are unit-tested without a model.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::MySqlPool;
use std::collections::HashSet;
use ts_rs::TS;

/// Most recent past taggings to few-shot. Enough to teach a style; bounded so the
/// prompt stays a fixed, cacheable size as history grows.
pub const MAX_EXAMPLES: u32 = 80;
/// The most suggestions to surface — a short, glanceable head of the list.
pub const MAX_SUGGESTIONS: usize = 6;
/// How many to keep in the cache. More than are shown, because the ones already
/// chosen are dropped at display time: caching exactly six would mean a check-in
/// where you'd picked three of them showed only three.
pub const MAX_CACHED: usize = 12;

/// Request from the picker: which check-in this is, the note, the whole feelings
/// vocabulary as candidates, and the tokens already chosen.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsRequest {
    /// The check-in being edited — the cache key, so reopening the picker on an
    /// unchanged note costs a lookup rather than a fresh generation.
    pub ulid: String,
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
/// the request's candidates and not already chosen — plus enough state for the
/// picker to be honest about where they came from.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsResponse {
    pub suggestions: Vec<String>,
    /// These were computed from an EARLIER wording of the note. Worth showing —
    /// they're usually still close — but only if labelled as such.
    pub stale: bool,
    /// A generation for the current wording is outstanding, so a better answer is
    /// coming. False when nothing is running, including when no worker exists at
    /// all: the picker must never claim to be thinking when nothing is.
    pub pending: bool,
    /// Seconds since that generation was queued, for "thinking for 12s". Counted
    /// from the queue, not from this request, so it survives closing and
    /// reopening the picker. Present only while `pending`.
    pub thinking_secs: Option<u32>,
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
/// past taggings as few-shot. The whole thing is fixed between check-ins, so the
/// worker's model can cache its KV prefix and only re-read the note.
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
            s.push_str(&serde_json::to_string(&e.tokens).expect("Vec<String> always serialises"));
            s.push('\n');
        }
    }
    s
}

/// The user turn: the note, and the shape of the answer.
pub fn build_user(note: &str) -> String {
    format!(
        "Note:\n{note}\n\nReturn JSON {{\"tokens\": [up to {MAX_SUGGESTIONS} tokens copied exactly \
         from the list, most-fitting first]}}."
    )
}

/// The whole prompt as the worker receives it — self-contained, so the worker
/// needs no database and no copy of the vocabulary.
pub fn build_prompt(
    candidates: &[EmotionCandidate],
    examples: &[EmotionExample],
    note: &str,
) -> serde_json::Value {
    serde_json::json!({
        "system": build_system(candidates, examples),
        "user": build_user(note),
    })
}

/// Identify a note by content. Comparing hashes rather than text keeps the
/// cache row small and makes "is this still the wording those came from?" a
/// fixed-cost check. Whitespace-trimmed, so re-opening after a stray space does
/// not look like an edit.
pub fn note_hash(note: &str) -> String {
    hex::encode(Sha256::digest(note.trim().as_bytes()))
}

/// Fetch the user's own labelled check-ins (note + chosen feelings) for the
/// few-shot, most recent first — but only through the end of YESTERDAY (UTC).
///
/// Excluding today is what keeps the few-shot, and therefore the whole system
/// prompt, byte-identical for a day. That stability is what lets the worker's
/// model cache the prompt's KV prefix to disk once and reuse it across the day's
/// requests (the prefill of that prefix is most of a request); a sliding
/// "latest 80" would shift on every new tagging and never hit the cache. The cost
/// is that a feeling tagged today does not inform today's suggestions until
/// tomorrow — negligible against 80 examples of history, and the whole point of
/// the cutoff. `id DESC` breaks `recorded_at` ties so the set — and thus the cache
/// key derived from the prompt — is deterministic.
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
           AND recorded_at < UTC_DATE() \
         ORDER BY recorded_at DESC, id DESC LIMIT ?",
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

/// Pull the ranked tokens out of what the model wrote — the JSON `{"tokens": [...]}`
/// it was asked for. Pure, so it is tested against captured output. Anything that
/// isn't that shape yields no tokens rather than an error: the model simply gave us
/// nothing usable, which is a legitimate (if disappointing) answer, not a failure
/// worth surfacing.
pub fn parse_tokens(content: &str) -> Vec<String> {
    // Models sometimes wrap JSON in a ```json fence despite being asked not to.
    let trimmed = content.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.rsplit_once("```"))
        .map(|(head, _)| head)
        .unwrap_or(trimmed);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
        return Vec::new();
    };
    v.get("tokens")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
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
