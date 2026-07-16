//! Two-user isolation against a real MariaDB (B4): every user's data being
//! invisible and untouchable to every other user is a core invariant of the
//! open-to-any-Nextcloud-user model, so it gets tests, not audits. One
//! sequential test (parallel tests contend on sync_rev and can deadlock).
//! Runs only when LIFE_TEST_DATABASE_URL is set; skips otherwise.

use chrono::{TimeZone, Utc};
use life::db;
use life::shopping::repo as shopping_repo;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, ShoppingDoc, TodoDoc, TodoLinkDoc, WellbeingDoc};
use life::todo::links;
use life::todo::repo as todo_repo;
use life::trash::TrashKind;
use life::trash::repo as trash_repo;

const A: &str = "test-user-iso-a";
const B: &str = "test-user-iso-b";

fn shopping_doc(ulid: &str, name: &str) -> ShoppingDoc {
    ShoppingDoc {
        ulid: ulid.into(),
        id: None,
        name: name.into(),
        quantity: None,
        unit: None,
        barcode: None,
        category: "food".into(),
        product_id: None,
        done: false,
        deleted: false,
        rev: 0,
    }
}

fn todo_doc(ulid: &str, title: &str) -> TodoDoc {
    TodoDoc {
        ulid: ulid.into(),
        id: None,
        title: title.into(),
        todo_type: "task".into(),
        status: "open".into(),
        priority: None,
        notes: None,
        not_before: None,
        due: None,
        shared: false,
        deleted: false,
        rev: 0,
    }
}

fn link_doc(ulid: &str, from: &str) -> TodoLinkDoc {
    TodoLinkDoc {
        ulid: ulid.into(),
        id: None,
        from: from.into(),
        kind: "related".into(),
        target_kind: "todo".into(),
        target_ref: "01ISOTARGET000000000000000".into(),
        deleted: false,
        rev: 0,
    }
}

/// `score` is in tenths (20 = a 2).
fn wellbeing_doc(ulid: &str, score: u8) -> WellbeingDoc {
    WellbeingDoc {
        ulid: ulid.into(),
        id: None,
        recorded_at: Utc.with_ymd_and_hms(2026, 7, 9, 8, 0, 0).unwrap(),
        score_tenths: score,
        energy_tenths: None,
        emotions: vec![],
        note: None,
        deleted: false,
        rev: 0,
    }
}

fn entry<D>(doc: D) -> PushEntry<D> {
    PushEntry {
        new_document_state: doc,
        assumed_master_state: None,
    }
}

#[tokio::test]
async fn two_users_cannot_see_or_touch_each_others_data() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping isolation DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    for table in ["shopping_items", "todos", "todo_links", "wellbeing"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE user_id IN (?, ?)"
        )))
        .bind(A)
        .bind(B)
        .execute(&pool)
        .await
        .unwrap();
    }

    // One doc per user per collection, via the sync push (the surface under test).
    let sa = "01ISOSHOPAAAAAAAAAAAAAAAAA";
    let sb = "01ISOSHOPBBBBBBBBBBBBBBBBB";
    let ta = "01ISOTODOAAAAAAAAAAAAAAAAA";
    let tb = "01ISOTODOBBBBBBBBBBBBBBBBB";
    let la = "01ISOLINKAAAAAAAAAAAAAAAAA";
    let lb = "01ISOLINKBBBBBBBBBBBBBBBBB";
    let wa = "01ISOWELLAAAAAAAAAAAAAAAAA";
    let wb = "01ISOWELLBBBBBBBBBBBBBBBBB";
    sync_repo::push_shopping(&pool, A, vec![entry(shopping_doc(sa, "A's milk"))])
        .await
        .unwrap();
    sync_repo::push_shopping(&pool, B, vec![entry(shopping_doc(sb, "B's milk"))])
        .await
        .unwrap();
    sync_repo::push_todo(&pool, A, vec![entry(todo_doc(ta, "A's task"))])
        .await
        .unwrap();
    sync_repo::push_todo(&pool, B, vec![entry(todo_doc(tb, "B's task"))])
        .await
        .unwrap();
    sync_repo::push_todo_link(&pool, A, vec![entry(link_doc(la, ta))])
        .await
        .unwrap();
    sync_repo::push_todo_link(&pool, B, vec![entry(link_doc(lb, tb))])
        .await
        .unwrap();
    sync_repo::push_wellbeing(&pool, A, vec![entry(wellbeing_doc(wa, 40))])
        .await
        .unwrap();
    sync_repo::push_wellbeing(&pool, B, vec![entry(wellbeing_doc(wb, 20))])
        .await
        .unwrap();

    // Pull is scoped: A sees exactly A's doc, never B's — per collection.
    let pulled = sync_repo::pull_shopping(&pool, A, 0, 100).await.unwrap();
    assert!(pulled.documents.iter().any(|d| d.ulid == sa));
    assert!(!pulled.documents.iter().any(|d| d.ulid == sb));
    let pulled = sync_repo::pull_todo(&pool, A, 0, 100).await.unwrap();
    assert!(pulled.documents.iter().any(|d| d.ulid == ta));
    assert!(!pulled.documents.iter().any(|d| d.ulid == tb));
    let pulled = sync_repo::pull_todo_link(&pool, A, 0, 100).await.unwrap();
    assert!(pulled.documents.iter().any(|d| d.ulid == la));
    assert!(!pulled.documents.iter().any(|d| d.ulid == lb));
    let pulled = sync_repo::pull_wellbeing(&pool, A, 0, 100).await.unwrap();
    assert!(pulled.documents.iter().any(|d| d.ulid == wa));
    assert!(!pulled.documents.iter().any(|d| d.ulid == wb));

    // Push onto another user's ulid cannot touch their row. The push sees no
    // row for (ulid, A) and takes the insert path; ulids are globally UNIQUE
    // per table, so the insert collides and surfaces as a loud error (a 500 at
    // the route), never as a silent cross-user write.
    let res = sync_repo::push_shopping(&pool, A, vec![entry(shopping_doc(sb, "hijack"))]).await;
    assert!(res.is_err(), "cross-user shopping push must fail loudly");
    let res = sync_repo::push_todo(&pool, A, vec![entry(todo_doc(tb, "hijack"))]).await;
    assert!(res.is_err(), "cross-user todo push must fail loudly");
    let res = sync_repo::push_todo_link(&pool, A, vec![entry(link_doc(lb, ta))]).await;
    assert!(res.is_err(), "cross-user link push must fail loudly");
    let res = sync_repo::push_wellbeing(&pool, A, vec![entry(wellbeing_doc(wb, 10))]).await;
    assert!(res.is_err(), "cross-user wellbeing push must fail loudly");

    // ... and B's rows are untouched by the attempts.
    let after = sync_repo::pull_shopping(&pool, B, 0, 100).await.unwrap();
    let doc = after.documents.iter().find(|d| d.ulid == sb).unwrap();
    assert_eq!(doc.name, "B's milk");
    let after = sync_repo::pull_todo(&pool, B, 0, 100).await.unwrap();
    let doc = after.documents.iter().find(|d| d.ulid == tb).unwrap();
    assert_eq!(doc.title, "B's task");
    let after = sync_repo::pull_wellbeing(&pool, B, 0, 100).await.unwrap();
    let doc = after.documents.iter().find(|d| d.ulid == wb).unwrap();
    assert_eq!(doc.score_tenths, 20);

    // REST list surfaces are scoped the same way.
    let list = shopping_repo::list(&pool, A).await.unwrap();
    assert!(list.iter().all(|i| i.name != "B's milk"));
    let list = todo_repo::list(&pool, A).await.unwrap();
    assert!(list.iter().all(|t| t.title != "B's task"));
    let list = links::list(&pool, A).await.unwrap();
    assert!(
        list.iter().all(|l| l.from != tb),
        "A must not see B's links"
    );

    // Trash is scoped: B tombstones an item; A neither sees it in trash nor
    // can restore it. B can.
    let cur = sync_repo::pull_shopping(&pool, B, 0, 100).await.unwrap();
    let doc = cur.documents.iter().find(|d| d.ulid == sb).unwrap().clone();
    let mut del = doc.clone();
    del.deleted = true;
    sync_repo::push_shopping(
        &pool,
        B,
        vec![PushEntry {
            new_document_state: del,
            assumed_master_state: Some(doc),
        }],
    )
    .await
    .unwrap();
    let trash_a = trash_repo::list(&pool, A).await.unwrap();
    assert!(trash_a.iter().all(|t| t.ref_ != sb));
    assert!(
        !trash_repo::restore(&pool, A, TrashKind::Shopping, sb)
            .await
            .unwrap(),
        "restore as the wrong user must be a no-op"
    );
    assert!(
        trash_repo::restore(&pool, B, TrashKind::Shopping, sb)
            .await
            .unwrap()
    );
}
