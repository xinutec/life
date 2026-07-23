//! Storage for emotion suggestions: the per-check-in cache, and the queue the
//! Mac's worker drains.
//!
//! The split of responsibility is the point. This module never talks to a model;
//! it decides only *what is known* about a check-in's feelings (the last computed
//! set and which wording produced it) and *what still needs computing*. Whether a
//! model exists at all is the caller's problem — the queue is happy to hold work
//! for a worker that isn't running yet, and the results simply appear later.

use sqlx::MySqlPool;

/// A stale claim is retried rather than left to rot, so a worker that dies
/// mid-generation costs one window, not the note forever.
const CLAIM_STALE_SECS: i64 = 120;

/// Decode a stored JSON column into `T`. A column that fails to parse is corrupt
/// stored data — surface it as a decode error, never default it to empty, or a
/// bad row would read as "the user has no feelings here" and the loss would be
/// silent.
fn decode<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> sqlx::Result<T> {
    serde_json::from_value(v).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

/// What we already know about a check-in's feelings.
pub struct Cached {
    /// The wording these came from — compare with the current note's hash to
    /// find out whether they are fresh or merely the previous answer.
    pub note_hash: String,
    pub tokens: Vec<String>,
}

/// A queued generation, from the point of view of someone waiting on it.
pub struct Queued {
    /// Seconds since this wording was first queued.
    pub thinking_secs: i64,
}

/// A job as the worker sees it: an id to report against, and a self-contained
/// prompt. Deliberately carries no user id or note — the worker generates text
/// and has no business knowing whose feelings it is guessing at.
pub struct Job {
    pub id: u64,
    pub prompt: serde_json::Value,
}

/// The last computed suggestions for a check-in, whatever wording produced them.
pub async fn cached(pool: &MySqlPool, user_id: &str, ulid: &str) -> sqlx::Result<Option<Cached>> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT note_hash, tokens FROM emotion_suggestions WHERE user_id = ? AND ulid = ?",
    )
    .bind(user_id)
    .bind(ulid)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((note_hash, tokens)) => Ok(Some(Cached {
            note_hash,
            tokens: decode(tokens)?,
        })),
        None => Ok(None),
    }
}

/// How long the job for this exact wording has been waiting, if one is queued.
///
/// Checked before anything is built, because the picker asks every couple of
/// seconds while it waits: without this, each poll would re-read eighty past
/// check-ins and rebuild a three-thousand-token prompt to arrive at a row that
/// is already sitting there.
pub async fn pending_for(
    pool: &MySqlPool,
    user_id: &str,
    ulid: &str,
    note_hash: &str,
) -> sqlx::Result<Option<Queued>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT UNIX_TIMESTAMP(NOW()) - UNIX_TIMESTAMP(created_at) FROM emotion_jobs \
         WHERE user_id = ? AND ulid = ? AND note_hash = ?",
    )
    .bind(user_id)
    .bind(ulid)
    .bind(note_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(secs,)| Queued {
        thinking_secs: secs.max(0),
    }))
}

/// Queue a generation for this wording, and say how long it has been waiting.
///
/// Asking again for the SAME wording (reopening the picker) must not restart the
/// clock — the honest answer to "how long has it been thinking" is measured from
/// when the work was first queued. A genuinely new wording replaces the pending
/// job in place, clock included: the old one is now answering a question nobody
/// is asking.
pub async fn enqueue(
    pool: &MySqlPool,
    user_id: &str,
    ulid: &str,
    note_hash: &str,
    prompt: &serde_json::Value,
    candidates: &[String],
) -> sqlx::Result<Queued> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT note_hash FROM emotion_jobs WHERE user_id = ? AND ulid = ?")
            .bind(user_id)
            .bind(ulid)
            .fetch_optional(pool)
            .await?;

    match existing {
        Some((hash,)) if hash == note_hash => {}
        Some(_) => {
            sqlx::query(
                "UPDATE emotion_jobs \
                 SET note_hash = ?, prompt = ?, candidates = ?, created_at = NOW(), taken_at = NULL \
                 WHERE user_id = ? AND ulid = ?",
            )
            .bind(note_hash)
            .bind(prompt)
            .bind(serde_json::json!(candidates))
            .bind(user_id)
            .bind(ulid)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO emotion_jobs (user_id, ulid, note_hash, prompt, candidates) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(ulid)
            .bind(note_hash)
            .bind(prompt)
            .bind(serde_json::json!(candidates))
            .execute(pool)
            .await?;
        }
    }

    // Age is measured by the database's clock on both ends, so it can't be skewed
    // by the pod's and the caller's clocks disagreeing.
    let (secs,): (i64,) = sqlx::query_as(
        "SELECT UNIX_TIMESTAMP(NOW()) - UNIX_TIMESTAMP(created_at) FROM emotion_jobs \
         WHERE user_id = ? AND ulid = ?",
    )
    .bind(user_id)
    .bind(ulid)
    .fetch_one(pool)
    .await?;
    Ok(Queued {
        thinking_secs: secs.max(0),
    })
}

/// Claim the oldest unclaimed job (or one whose claim has gone stale). Oldest
/// first, because the person waiting longest should be served first — and with a
/// single worker that is also simply the order the notes were written in.
pub async fn claim_next(pool: &MySqlPool) -> sqlx::Result<Option<Job>> {
    let row: Option<(u64, serde_json::Value)> = sqlx::query_as(
        "SELECT id, prompt FROM emotion_jobs \
         WHERE taken_at IS NULL OR taken_at < NOW() - INTERVAL ? SECOND \
         ORDER BY created_at LIMIT 1",
    )
    .bind(CLAIM_STALE_SECS)
    .fetch_optional(pool)
    .await?;
    let Some((id, prompt)) = row else {
        return Ok(None);
    };
    // Claim it by compare-and-set, repeating the condition it was selected under.
    // Two polls can overlap — the worker reconnecting while its previous request
    // is still in flight — and a plain `SET taken_at = NOW()` would hand both the
    // same job. Losing the race means someone else got there first: report
    // nothing rather than a job that is already being worked on.
    let claimed = sqlx::query(
        "UPDATE emotion_jobs SET taken_at = NOW() \
         WHERE id = ? AND (taken_at IS NULL OR taken_at < NOW() - INTERVAL ? SECOND)",
    )
    .bind(id)
    .bind(CLAIM_STALE_SECS)
    .execute(pool)
    .await?;
    if claimed.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(Job { id, prompt }))
}

/// The candidate vocabulary a job was asked about — the guardrail its answer is
/// checked against. Read at completion rather than trusted from the worker, so a
/// worker cannot widen the vocabulary it is allowed to answer with.
pub struct Completion {
    pub user_id: String,
    pub ulid: String,
    pub note_hash: String,
    pub candidates: Vec<String>,
}

pub async fn job_for_completion(pool: &MySqlPool, id: u64) -> sqlx::Result<Option<Completion>> {
    let row: Option<(String, String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT user_id, ulid, note_hash, candidates FROM emotion_jobs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((user_id, ulid, note_hash, candidates)) => Ok(Some(Completion {
            user_id,
            ulid,
            note_hash,
            candidates: decode(candidates)?,
        })),
        None => Ok(None),
    }
}

/// Record an answer and retire the job.
///
/// The write is conditional on the job still holding the wording it was queued
/// for: if the note changed while the model was busy, this answer is about text
/// that no longer exists and must not become the cached truth. The stored set
/// keeps every valid token (not just the handful shown), because which ones are
/// worth showing depends on what is selected at the time you look.
pub async fn complete(
    pool: &MySqlPool,
    id: u64,
    job: &Completion,
    tokens: &[String],
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    let still_current: Option<(String,)> =
        sqlx::query_as("SELECT note_hash FROM emotion_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if still_current.map(|(h,)| h) != Some(job.note_hash.clone()) {
        tx.commit().await?;
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO emotion_suggestions (user_id, ulid, note_hash, tokens, computed_at) \
         VALUES (?, ?, ?, ?, NOW()) \
         ON DUPLICATE KEY UPDATE note_hash = VALUES(note_hash), tokens = VALUES(tokens), \
                                 computed_at = VALUES(computed_at)",
    )
    .bind(&job.user_id)
    .bind(&job.ulid)
    .bind(&job.note_hash)
    .bind(serde_json::json!(tokens))
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM emotion_jobs WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
