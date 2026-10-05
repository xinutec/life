//! Emotion suggestions: the per-check-in cache and the queue the Mac's worker
//! drains. Never talks to a model; the queue holds work for a worker not yet up.

use sqlx::MySqlPool;

/// After this a claim is retried, so a dead worker costs one window. Well over the
/// longest real generation (~145 s), so a live worker's claim also proves it alive.
const CLAIM_STALE_SECS: i64 = 180;

/// A column that fails to parse is an error, never empty: that would read as "no
/// feelings here".
fn decode<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> sqlx::Result<T> {
    serde_json::from_value(v).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

pub struct Cached {
    /// Compared with the current note's hash: fresh, or the previous answer.
    pub note_hash: String,
    pub tokens: Vec<String>,
}

pub struct Queued {
    pub thinking_secs: i64,
    /// A worker holds a fresh claim: it is generating now. The liveness signal that
    /// survives a long generation, when the worker cannot poll.
    pub being_worked: bool,
}

/// An id and a self-contained prompt; no user and no note: the worker has no
/// business knowing whose feelings these are.
pub struct Job {
    pub id: u64,
    pub prompt: serde_json::Value,
}

/// The last suggestions for a check-in, whatever wording produced them.
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

/// Checked before anything is built: the picker asks every couple of seconds, and
/// each ask would otherwise rebuild a three-thousand-token prompt.
pub async fn pending_for(
    pool: &MySqlPool,
    user_id: &str,
    ulid: &str,
    note_hash: &str,
) -> sqlx::Result<Option<Queued>> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT UNIX_TIMESTAMP(NOW()) - UNIX_TIMESTAMP(created_at), \
                taken_at IS NOT NULL AND taken_at > NOW() - INTERVAL ? SECOND \
         FROM emotion_jobs \
         WHERE user_id = ? AND ulid = ? AND note_hash = ?",
    )
    .bind(CLAIM_STALE_SECS)
    .bind(user_id)
    .bind(ulid)
    .bind(note_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(secs, claimed)| Queued {
        thinking_secs: secs.max(0),
        being_worked: claimed != 0,
    }))
}

/// Queue this wording. Asking again for the same wording keeps the clock; a new
/// wording replaces the job, clock included.
pub async fn enqueue(
    pool: &MySqlPool,
    user_id: &str,
    ulid: &str,
    note_hash: &str,
    prompt: &serde_json::Value,
    candidates: &[String],
) -> sqlx::Result<Queued> {
    // One statement, so two first asks cannot both insert. Assignments run left
    // to right: `note_hash` last, so the comparisons before it see the queued one.
    sqlx::query(
        "INSERT INTO emotion_jobs (user_id, ulid, note_hash, prompt, candidates) \
         VALUES (?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE \
         prompt = IF(note_hash = VALUES(note_hash), prompt, VALUES(prompt)), \
         candidates = IF(note_hash = VALUES(note_hash), candidates, VALUES(candidates)), \
         created_at = IF(note_hash = VALUES(note_hash), created_at, NOW()), \
         taken_at = IF(note_hash = VALUES(note_hash), taken_at, NULL), \
         note_hash = VALUES(note_hash)",
    )
    .bind(user_id)
    .bind(ulid)
    .bind(note_hash)
    .bind(prompt)
    .bind(serde_json::json!(candidates))
    .execute(pool)
    .await?;

    // The database's clock on both ends.
    let (secs, claimed): (i64, i64) = sqlx::query_as(
        "SELECT UNIX_TIMESTAMP(NOW()) - UNIX_TIMESTAMP(created_at), \
                taken_at IS NOT NULL AND taken_at > NOW() - INTERVAL ? SECOND \
         FROM emotion_jobs WHERE user_id = ? AND ulid = ?",
    )
    .bind(CLAIM_STALE_SECS)
    .bind(user_id)
    .bind(ulid)
    .fetch_one(pool)
    .await?;
    Ok(Queued {
        thinking_secs: secs.max(0),
        being_worked: claimed != 0,
    })
}

/// The oldest unclaimed (or stale) job: whoever has waited longest.
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
    // Compare-and-set on the selecting condition: overlapping polls would
    // otherwise both get the job.
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

/// Read here rather than trusted from the worker, which may not widen it.
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

/// Only while the job still holds the wording it was queued for: an answer about
/// text that no longer exists must not be cached. Every valid token is kept, as
/// which to show depends on the selection at the time.
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

/// For rebuilding the day's prompt later (0038); a cache, never an authority.
pub async fn remember_vocabulary(
    pool: &MySqlPool,
    user_id: &str,
    candidates: &[crate::wellbeing::suggest::EmotionCandidate],
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO emotion_vocabulary (user_id, candidates) VALUES (?, ?) \
         ON DUPLICATE KEY UPDATE candidates = VALUES(candidates)",
    )
    .bind(user_id)
    .bind(serde_json::json!(candidates))
    .execute(pool)
    .await?;
    Ok(())
}

/// One row: the warm slot holds a single prompt, which fits one user.
pub async fn latest_vocabulary(
    pool: &MySqlPool,
) -> sqlx::Result<Option<(String, Vec<crate::wellbeing::suggest::EmotionCandidate>)>> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT user_id, candidates FROM emotion_vocabulary ORDER BY updated_at DESC, user_id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    match row {
        Some((user_id, candidates)) => Ok(Some((user_id, decode(candidates)?))),
        None => Ok(None),
    }
}
