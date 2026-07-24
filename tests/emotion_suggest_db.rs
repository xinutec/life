//! The emotion-suggestion cache and job queue against a real MariaDB. Runs only
//! when LIFE_TEST_DATABASE_URL is set; skips otherwise.
//!
//! What's worth pinning down here is the behaviour a person actually feels:
//! reopening the picker doesn't restart the clock, editing the note does, and an
//! answer that arrives after the note moved on is discarded rather than shown as
//! if it described the new text.

use life::db;
use life::wellbeing::suggest_store as store;

fn prompt(note: &str) -> serde_json::Value {
    serde_json::json!({ "system": "…", "user": note })
}

const VOCAB: [&str; 3] = ["Sad/Low", "Sad/Empty", "Happy/Calm"];

fn vocab() -> Vec<String> {
    VOCAB.iter().map(|s| s.to_string()).collect()
}

/// Both tables are pure derived data, so a test owns its user's rows outright.
async fn wipe(pool: &sqlx::MySqlPool, user: &str) {
    sqlx::query("DELETE FROM emotion_jobs WHERE user_id = ?")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM emotion_suggestions WHERE user_id = ?")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn suggestion_cache_and_queue_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping emotion suggestion DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-emotion-suggest";
    let ulid = "01J0000000000000000000TEST";
    wipe(&pool, user).await;

    // Nothing known about a check-in nobody has asked about.
    assert!(store::cached(&pool, user, ulid).await.unwrap().is_none());

    // First ask queues the work.
    let first = store::enqueue(&pool, user, ulid, "hash-a", &prompt("a"), &vocab())
        .await
        .unwrap();
    // Tolerant of straddling a second boundary between the insert and the read.
    assert!(
        first.thinking_secs <= 1,
        "just queued: {}",
        first.thinking_secs
    );

    // Asking again for the SAME wording (reopening the picker) must not queue a
    // second job, and must not restart the clock.
    sqlx::query(
        "UPDATE emotion_jobs SET created_at = NOW() - INTERVAL 12 SECOND WHERE user_id = ?",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let again = store::enqueue(&pool, user, ulid, "hash-a", &prompt("a"), &vocab())
        .await
        .unwrap();
    assert!(
        (12..=13).contains(&again.thinking_secs),
        "the same wording keeps its original clock, got {}",
        again.thinking_secs
    );
    let (jobs,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM emotion_jobs WHERE user_id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 1, "one job per check-in, replaced in place");

    // The worker takes it and answers.
    let job = store::claim_next(&pool).await.unwrap().expect("a job");
    let done = store::job_for_completion(&pool, job.id)
        .await
        .unwrap()
        .expect("job row");
    store::complete(&pool, job.id, &done, &["Sad/Low".into()])
        .await
        .unwrap();

    let hit = store::cached(&pool, user, ulid)
        .await
        .unwrap()
        .expect("cached");
    assert_eq!(hit.note_hash, "hash-a");
    assert_eq!(hit.tokens, vec!["Sad/Low"]);
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM emotion_jobs WHERE user_id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "an answered job is retired");

    // Editing the note queues fresh work with a fresh clock — and the previous
    // answer stays put, since it is what the picker shows while this one runs.
    let edited = store::enqueue(&pool, user, ulid, "hash-b", &prompt("b"), &vocab())
        .await
        .unwrap();
    assert!(
        edited.thinking_secs <= 1,
        "a new wording restarts the clock, got {}",
        edited.thinking_secs
    );
    let still = store::cached(&pool, user, ulid)
        .await
        .unwrap()
        .expect("cached");
    assert_eq!(
        still.note_hash, "hash-a",
        "the earlier answer survives to be shown while the new one is computed"
    );

    // An answer that arrives after the note moved on again is dropped: it
    // describes text that no longer exists.
    let stale_job = store::claim_next(&pool).await.unwrap().expect("a job");
    let stale = store::job_for_completion(&pool, stale_job.id)
        .await
        .unwrap()
        .expect("job row");
    store::enqueue(&pool, user, ulid, "hash-c", &prompt("c"), &vocab())
        .await
        .unwrap();
    store::complete(&pool, stale_job.id, &stale, &["Happy/Calm".into()])
        .await
        .unwrap();
    let unchanged = store::cached(&pool, user, ulid)
        .await
        .unwrap()
        .expect("cached");
    assert_eq!(
        unchanged.note_hash, "hash-a",
        "a late answer must not overwrite the cache with the wrong wording"
    );
    let (pending,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM emotion_jobs WHERE user_id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 1, "the newest wording is still queued");

    // A job the model could not answer is recorded as an empty answer, not
    // dropped: dropping it would leave the note uncached and the picker would
    // queue the same failing work again on its next poll.
    let doomed = store::claim_next(&pool).await.unwrap().expect("a job");
    let failed = store::job_for_completion(&pool, doomed.id)
        .await
        .unwrap()
        .expect("job row");
    store::complete(&pool, doomed.id, &failed, &[])
        .await
        .unwrap();
    let (none,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM emotion_jobs WHERE user_id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(none, 0);
    let gave_up = store::cached(&pool, user, ulid)
        .await
        .unwrap()
        .expect("cached");
    assert_eq!(
        gave_up.note_hash, "hash-c",
        "the failure is remembered per wording"
    );
    assert!(gave_up.tokens.is_empty());

    wipe(&pool, user).await;
}

/// The picker must keep waiting through a long generation. A worker blocked on the
/// model for ~100-145s cannot poll, so the "seen recently" clock goes stale — but
/// the claim it holds proves it is alive and working. `being_worked` is that
/// proof, and it is what keeps `pending` true past the poll-liveness window.
#[tokio::test]
async fn a_claimed_job_reads_as_being_worked_until_the_claim_goes_stale() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping being-worked DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-emotion-beingworked";
    let ulid = "01J0000000000000000000WORK";
    wipe(&pool, user).await;

    // Queued but unclaimed: no worker is on it yet, so nothing is "being worked".
    store::enqueue(&pool, user, ulid, "hash-a", &prompt("a"), &vocab())
        .await
        .unwrap();
    let unclaimed = store::pending_for(&pool, user, ulid, "hash-a")
        .await
        .unwrap()
        .expect("queued");
    assert!(
        !unclaimed.being_worked,
        "a job no worker has claimed is not being worked"
    );

    // A worker claims it (taken_at set) — now it is being worked, and stays so a
    // hundred seconds in, well past the 90s poll-liveness window, because the
    // claim itself is the liveness signal that survives a blocked generation.
    for ago in [0_i64, 100] {
        sqlx::query(
            "UPDATE emotion_jobs SET taken_at = NOW() - INTERVAL ? SECOND WHERE user_id = ?",
        )
        .bind(ago)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
        let working = store::pending_for(&pool, user, ulid, "hash-a")
            .await
            .unwrap()
            .expect("queued");
        assert!(
            working.being_worked,
            "a claim {ago}s old is still a live worker on the job"
        );
    }

    // A claim older than the stale window: the worker is presumed dead, the job is
    // free to be reclaimed, and the picker must stop promising an answer.
    sqlx::query("UPDATE emotion_jobs SET taken_at = NOW() - INTERVAL 200 SECOND WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let abandoned = store::pending_for(&pool, user, ulid, "hash-a")
        .await
        .unwrap()
        .expect("queued");
    assert!(
        !abandoned.being_worked,
        "a claim past the stale window no longer counts as being worked"
    );

    wipe(&pool, user).await;
}
