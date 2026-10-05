//! Emotion suggestions from a local model on the Mac, which the fleet cannot
//! dial: this builds a self-contained prompt, [`store`](super::suggest_store)
//! queues it, and the worker answers. A few-shot of the user's own taggings
//! teaches their calibration (*Low*, not *Grief*), which roughly doubled
//! agreement; [`filter_suggestions`] drops anything outside the candidates.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::MySqlPool;
use std::collections::HashSet;
use std::time::Duration;
use ts_rs::TS;

/// Bounded, so the prompt stays a fixed, cacheable size.
pub const MAX_EXAMPLES: u32 = 80;
pub const MAX_SUGGESTIONS: usize = 6;
/// More than are shown, as the chosen ones are dropped at display time.
pub const MAX_CACHED: usize = 12;

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsRequest {
    /// The cache key.
    pub ulid: String,
    pub note: String,
    pub candidates: Vec<EmotionCandidate>,
    #[serde(default)]
    pub already: Vec<String>,
}

/// A `Core/Name` token and its gloss; stored back so a rollover can rebuild the
/// prompt.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EmotionCandidate {
    pub token: String,
    pub desc: String,
}

/// Preload while a note is being written; the system prompt needs only the
/// vocabulary.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WarmEmotionsRequest {
    pub candidates: Vec<EmotionCandidate>,
}

/// Suggested tokens, best first, each a candidate and not already chosen.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestEmotionsResponse {
    pub suggestions: Vec<String>,
    /// From an earlier wording: usually close, but labelled.
    pub stale: bool,
    /// A better answer is coming. Never true with no worker to produce it.
    pub pending: bool,
    /// Counted from the queue, so it survives reopening; only while `pending`.
    pub thinking_secs: Option<u32>,
}

pub struct EmotionExample {
    pub note: String,
    pub tokens: Vec<String>,
}

const SYSTEM_INSTRUCTIONS: &str = "You help someone tag a wellbeing check-in with the feelings that match what they wrote. \
You are given the note and a fixed list of feelings, each as `token — meaning`. \
Choose the feelings from the list that are genuinely present in the note, ranked most-fitting first. \
Only choose feelings that are really there: returning few, or none, is correct — do not pad. \
Never invent a feeling; every token you return must be copied exactly from the list.";

/// Instructions, candidates, then past taggings: fixed between check-ins, so the
/// model's cached prefix holds and only the note is new.
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

pub fn build_user(note: &str) -> String {
    format!(
        "Note:\n{note}\n\nReturn JSON {{\"tokens\": [up to {MAX_SUGGESTIONS} tokens copied exactly \
         from the list, most-fitting first]}}."
    )
}

/// Self-contained: the worker needs no database and no vocabulary.
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

/// A note by content, trimmed, so a stray space is not an edit.
pub fn note_hash(note: &str) -> String {
    hex::encode(Sha256::digest(note.trim().as_bytes()))
}

/// The user's tagged check-ins, newest first, through YESTERDAY (UTC) only: the
/// prompt stays byte-identical all day for the prefix cache. `id DESC` makes the
/// set deterministic.
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

/// A few minutes past midnight, so the database has surely turned the day.
pub const ROLLOVER_WARM_AFTER_MIDNIGHT: Duration = Duration::from_secs(5 * 60);

/// Always positive: landing on the instant schedules the next day.
pub fn until_rollover_warm(now: DateTime<Utc>) -> Duration {
    let target = |day: NaiveDate| {
        day.and_time(NaiveTime::MIN).and_utc()
            + chrono::Duration::from_std(ROLLOVER_WARM_AFTER_MIDNIGHT).expect("5 minutes fits")
    };
    let today = target(now.date_naive());
    let next = if today > now {
        today
    } else {
        target(
            now.date_naive()
                .succ_opt()
                .expect("a day after today exists"),
        )
    };
    (next - now)
        .to_std()
        .unwrap_or(ROLLOVER_WARM_AFTER_MIDNIGHT)
}

/// The tokens from `{"tokens": [...]}`; any other shape is no tokens, which is an
/// answer, not an error.
pub fn parse_tokens(content: &str) -> Vec<String> {
    // Models wrap JSON in a ```json fence despite being asked not to.
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

/// The guardrail: a token outside the vocabulary never survives. Deduplicated in
/// rank order, capped at `max`.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Computed from this very wording.
    Answered,
    /// Not, or none: a job for this wording is queued.
    Waiting {
        secs: i64,
        /// A worker holds a fresh claim: generating now.
        being_worked: bool,
        /// A worker polled recently and will take the job.
        worker_alive: bool,
    },
}

/// What the picker is told. Pure. An earlier wording's suggestions are shown
/// marked `stale`; `pending` only with a worker there to answer.
pub fn respond(
    cached: Vec<String>,
    progress: Progress,
    candidates: &[EmotionCandidate],
    already: &[String],
) -> SuggestEmotionsResponse {
    // The cache holds every valid token; what to offer depends on the selection.
    let valid: HashSet<&str> = candidates.iter().map(|c| c.token.as_str()).collect();
    let already: HashSet<&str> = already.iter().map(String::as_str).collect();
    let suggestions = filter_suggestions(cached, &valid, &already, MAX_SUGGESTIONS);
    match progress {
        Progress::Answered => SuggestEmotionsResponse {
            suggestions,
            stale: false,
            pending: false,
            thinking_secs: None,
        },
        Progress::Waiting {
            secs,
            being_worked,
            worker_alive,
        } => {
            let pending = being_worked || worker_alive;
            SuggestEmotionsResponse {
                stale: !suggestions.is_empty(),
                suggestions,
                pending,
                thinking_secs: pending.then(|| u32::try_from(secs.max(0)).unwrap_or(u32::MAX)),
            }
        }
    }
}
