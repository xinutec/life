//! To-do list against a real MariaDB. Runs only when LIFE_TEST_DATABASE_URL is
//! set; skips otherwise. Covers repo CRUD (types + status + soft-delete) and a
//! sync pull/push round-trip (the offline path).

use chrono::NaiveDate;
use life::db;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, TodoDoc};
use life::todo::repo;
use life::todo::types::{NewTodo, TodoPriority, TodoStatus, TodoType, UpdateTodo};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[tokio::test]
async fn todo_crud_and_sync_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping todo DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-todo";
    sqlx::query("DELETE FROM todos WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Create two typed to-dos.
    let milk = repo::create(
        &pool,
        user,
        NewTodo {
            title: "Buy milk".into(),
            todo_type: TodoType::Purchase,
            priority: None,
            notes: None,
            not_before: None,
            due: None,
            shared: false,
        },
    )
    .await
    .unwrap();
    repo::create(
        &pool,
        user,
        NewTodo {
            title: "Call dentist".into(),
            todo_type: TodoType::Call,
            priority: Some(TodoPriority::High),
            notes: Some("re-book cleaning".into()),
            not_before: None,
            due: Some(date(2026, 7, 10)),
            shared: true,
        },
    )
    .await
    .unwrap();

    let all = repo::list(&pool, user).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(milk.status, TodoStatus::Open);
    assert_eq!(milk.todo_type, TodoType::Purchase);
    assert_eq!(milk.priority, None);
    assert_eq!(milk.due, None);
    let dentist = all.iter().find(|t| t.title == "Call dentist").unwrap();
    assert_eq!(dentist.priority, Some(TodoPriority::High));
    assert_eq!(dentist.due, Some(date(2026, 7, 10)));
    // `shared` round-trips: private by default (milk), opt-in (dentist).
    assert!(!milk.shared);
    assert!(dentist.shared);

    // Update: mark done, set a priority, change notes, add timing.
    let done = repo::update(
        &pool,
        user,
        milk.id,
        UpdateTodo {
            title: Some(milk.title.clone()),
            todo_type: Some(TodoType::Purchase),
            status: Some(TodoStatus::Done),
            priority: Some(Some(TodoPriority::Medium)),
            notes: Some(Some("got oat milk".into())),
            not_before: Some(Some(date(2026, 7, 5))),
            due: Some(Some(date(2026, 7, 20))),
            shared: Some(true),
        },
    )
    .await
    .unwrap()
    .expect("exists");
    assert_eq!(done.status, TodoStatus::Done);
    assert_eq!(done.priority, Some(TodoPriority::Medium));
    assert_eq!(done.notes.as_deref(), Some("got oat milk"));
    assert_eq!(done.not_before, Some(date(2026, 7, 5)));
    assert_eq!(done.due, Some(date(2026, 7, 20)));
    assert!(done.shared, "update flips shared on");

    // Soft delete hides it from reads.
    assert!(repo::delete(&pool, user, milk.id).await.unwrap());
    let after = repo::list(&pool, user).await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].title, "Call dentist");

    // Sync pull surfaces every row including the tombstone, in rev order.
    let pulled = sync_repo::pull_todo(&pool, user, 0, 100).await.unwrap();
    assert!(
        pulled
            .documents
            .iter()
            .any(|d| d.title == "Buy milk" && d.deleted)
    );
    assert!(
        pulled
            .documents
            .iter()
            .any(|d| d.title == "Call dentist" && !d.deleted)
    );

    // Sync push: a to-do created offline (client-minted ulid) lands on the server.
    let entry = PushEntry {
        new_document_state: TodoDoc {
            ulid: "0123456789ABCDEFGHJKMNPQRS".into(),
            id: None,
            title: "Pay rent".into(),
            todo_type: "call".into(),
            status: "open".into(),
            priority: Some("low".into()),
            notes: None,
            not_before: None,
            due: Some(date(2026, 8, 1)),
            shared: false,
            deleted: false,
            rev: 0,
        },
        assumed_master_state: None,
    };
    let conflicts = sync_repo::push_todo(&pool, user, vec![entry])
        .await
        .unwrap();
    assert!(conflicts.is_empty());
    let after_push = repo::list(&pool, user).await.unwrap();
    assert!(after_push.iter().any(|t| t.title == "Pay rent"
        && t.todo_type == TodoType::Call
        && t.priority == Some(TodoPriority::Low)
        && t.due == Some(date(2026, 8, 1))));
}

/// PATCH must be a *partial* update: an absent field leaves the stored value
/// alone, and an explicit `null` clears it. Before this, `UpdateTodo` required
/// title/type/status on every call, so sending just `{"notes": "..."}` was a 422
/// — the verb said PATCH but the payload had to be a whole to-do.
#[tokio::test]
async fn patch_leaves_absent_fields_alone_and_clears_on_null() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping partial-update test");
        return;
    };
    let pool = db::connect(&url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let user = "patch-partial-user";
    sqlx::query("DELETE FROM todos WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let orig = repo::create(
        &pool,
        user,
        NewTodo {
            title: "Raise scalp pain at the review".into(),
            todo_type: TodoType::Task,
            priority: Some(TodoPriority::Medium),
            notes: Some("old wording".into()),
            not_before: None,
            due: Some(date(2026, 7, 17)),
            shared: true,
        },
    )
    .await
    .unwrap();

    // Notes only: everything else must survive untouched.
    let patched = repo::update(
        &pool,
        user,
        orig.id,
        UpdateTodo {
            notes: Some(Some("new wording".into())),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .expect("exists");
    assert_eq!(patched.notes.as_deref(), Some("new wording"));
    assert_eq!(patched.title, orig.title, "absent title was overwritten");
    assert_eq!(
        patched.todo_type, orig.todo_type,
        "absent type was overwritten"
    );
    assert_eq!(patched.status, orig.status, "absent status was overwritten");
    assert_eq!(
        patched.priority, orig.priority,
        "absent priority was overwritten"
    );
    assert_eq!(patched.due, orig.due, "absent due was overwritten");
    assert!(patched.shared, "absent shared silently flipped to private");

    // Explicit null clears — distinct from absent.
    let cleared = repo::update(
        &pool,
        user,
        orig.id,
        UpdateTodo {
            due: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .expect("exists");
    assert_eq!(
        cleared.due, None,
        "explicit null did not clear the due date"
    );
    assert_eq!(
        cleared.notes.as_deref(),
        Some("new wording"),
        "clearing due touched notes"
    );
}

/// Merging server-side means PATCH writes back fields the caller never mentioned,
/// so it must read the row *inside* the transaction it writes in, with the row
/// locked. Reading on the pool first (a snapshot outside the transaction) makes a
/// concurrent write to an untouched field silently disappear: the merge restores
/// the value it read a moment earlier, and `next_rev()` stamps it as a legitimate
/// newer revision, so RxDB sees no conflict.
///
/// Deterministic, not timing-luck: a second transaction holds the row with
/// `SELECT … FOR UPDATE`. A plain (non-locking) read sails past that lock and gets
/// the stale row; a locking read blocks until the writer commits and sees the truth.
#[tokio::test]
async fn patch_does_not_revert_a_concurrent_write_to_an_untouched_field() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping patch-race test");
        return;
    };
    let pool = db::connect(&url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    const USER: &str = "patch-race-user";
    sqlx::query("DELETE FROM todos WHERE user_id = ?")
        .bind(USER)
        .execute(&pool)
        .await
        .unwrap();

    let orig = repo::create(
        &pool,
        USER,
        NewTodo {
            title: "Raise the eye watering at the review".into(),
            todo_type: TodoType::Task,
            priority: Some(TodoPriority::Medium),
            notes: Some("old wording".into()),
            not_before: None,
            due: None,
            shared: true,
        },
    )
    .await
    .unwrap();

    // A concurrent writer (think: an RxDB sync push from the phone) takes the row
    // and is mid-flight — it has the lock but has not committed yet.
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM todos WHERE id = ? FOR UPDATE")
        .bind(orig.id)
        .fetch_one(&mut *writer)
        .await
        .unwrap();

    // Meanwhile a PATCH arrives that touches ONLY the notes.
    let patch_pool = pool.clone();
    let id = orig.id;
    let patching = tokio::spawn(async move {
        repo::update(
            &patch_pool,
            USER,
            id,
            UpdateTodo {
                notes: Some(Some("new wording".into())),
                ..Default::default()
            },
        )
        .await
    });

    // Long enough for the PATCH to have done its read. If that read is a plain
    // SELECT on the pool, it is unblocked by the row lock and snapshots status=open.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // The concurrent writer now completes: the to-do is done.
    sqlx::query("UPDATE todos SET status = 'done' WHERE id = ?")
        .bind(orig.id)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();

    let patched = patching.await.unwrap().unwrap().expect("exists");
    assert_eq!(patched.notes.as_deref(), Some("new wording"));
    assert_eq!(
        patched.status,
        TodoStatus::Done,
        "PATCH wrote back status from a stale pre-lock read, reverting the concurrent completion"
    );

    // And in the database, not just the returned struct.
    let stored = repo::get(&pool, USER, orig.id)
        .await
        .unwrap()
        .expect("exists");
    assert_eq!(
        stored.status,
        TodoStatus::Done,
        "the concurrent completion was lost in the database"
    );
    assert_eq!(stored.notes.as_deref(), Some("new wording"));
}

/// The absent-vs-null distinction lives in serde, so pin it there too.
#[test]
fn patch_json_distinguishes_absent_from_null() {
    let absent: UpdateTodo = serde_json::from_str(r#"{"notes":"hi"}"#).unwrap();
    assert_eq!(absent.notes, Some(Some("hi".into())));
    assert_eq!(absent.due, None, "an omitted field must read as absent");
    assert_eq!(absent.title, None);

    let nulled: UpdateTodo = serde_json::from_str(r#"{"due":null}"#).unwrap();
    assert_eq!(
        nulled.due,
        Some(None),
        "an explicit null must read as clear-it"
    );
}
