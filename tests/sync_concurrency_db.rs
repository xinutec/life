//! Concurrent sync pushes against a real MariaDB. Each write takes the revision
//! counter before any row lock; taken the other way round, a new row's gap lock
//! and the counter deadlock between two pushes.

mod common;

use chrono::{TimeZone, Utc};
use life::db;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, WellbeingDoc};

fn doc(ulid: String) -> WellbeingDoc {
    WellbeingDoc {
        ulid,
        id: None,
        recorded_at: Utc.with_ymd_and_hms(2026, 7, 3, 9, 30, 0).unwrap(),
        score_tenths: 30,
        energy_tenths: None,
        emotions: vec![],
        note: None,
        deleted: false,
        rev: 0,
    }
}

#[tokio::test]
async fn concurrent_pushes_of_new_rows_never_deadlock() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    const DEVICES: usize = 12;
    const ROWS: usize = 8;
    sqlx::query("DELETE FROM wellbeing WHERE user_id LIKE 'test-user-sync-race-%'")
        .execute(&pool)
        .await
        .unwrap();

    let pushes: Vec<_> = (0..DEVICES)
        .map(|d| {
            let pool = pool.clone();
            tokio::spawn(async move {
                let user = format!("test-user-sync-race-{d}");
                for r in 0..ROWS {
                    // ULID-shaped and distinct, so every push inserts a new row.
                    let ulid = format!("01RACE{d:02}{r:02}{:0>16}", d * ROWS + r);
                    sync_repo::push_wellbeing(
                        &pool,
                        &user,
                        vec![PushEntry {
                            new_document_state: doc(ulid),
                            assumed_master_state: None,
                        }],
                    )
                    .await?;
                }
                anyhow::Ok(())
            })
        })
        .collect();
    // Spawned, so they already run concurrently; awaited in turn.
    for push in pushes {
        push.await.expect("task").expect("no push may fail");
    }
}

/// Replacing one source's dietary flags on many products at once. A delete of
/// rows that are not there takes a gap lock, which another product's insert can
/// need.
#[tokio::test]
async fn concurrent_fact_replaces_never_deadlock() {
    use life::products::ids::{Barcode, ExternalId};
    use life::products::nutrition::{Claim, DietaryFlag};
    use life::products::repo;
    use life::products::source::Source;

    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    const PRODUCTS: u64 = 12;
    let mut ids = Vec::new();
    for n in 0..PRODUCTS {
        let barcode: Barcode = format!("99977{n:08}").parse().unwrap();
        sqlx::query("DELETE FROM products WHERE barcode = ?")
            .bind(&barcode)
            .execute(&pool)
            .await
            .unwrap();
        let p = repo::upsert_external(
            &pool,
            Source::Off,
            &ExternalId::from(&barcode),
            Some(&barcode),
            &repo::ListingFields::default(),
        )
        .await
        .unwrap();
        ids.push(p.id);
    }

    let writes: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let pool = pool.clone();
            tokio::spawn(async move {
                for round in 0..6 {
                    let value = if round % 2 == 0 {
                        Claim::Yes
                    } else {
                        Claim::Maybe
                    };
                    let flags = [DietaryFlag {
                        flag: "vegan".parse().unwrap(),
                        value,
                    }];
                    repo::replace_dietary(&pool, id, &flags, Source::Asda).await?;
                }
                anyhow::Ok(())
            })
        })
        .collect();
    for write in writes {
        write.await.expect("task").expect("no write may fail");
    }
}

/// Pushing edits to a row while REST deletes and restores it. Both take the
/// counter first; a push taking the row first deadlocks with them.
#[tokio::test]
async fn pushes_and_trash_restores_on_one_row_never_deadlock() {
    use life::inventory::types::ItemCategory;
    use life::shopping::repo as shopping_repo;
    use life::sync::types::ShoppingDoc;

    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-sync-race-row";
    const ULID: &str = "01RACEROW0000000000000000A";
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let row = |name: &str| ShoppingDoc {
        ulid: ULID.into(),
        id: None,
        name: name.into(),
        quantity: None,
        unit: None,
        barcode: None,
        category: ItemCategory::Food,
        product_id: None,
        done: false,
        deleted: false,
        rev: 0,
    };
    sync_repo::push_shopping(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: row("Milk"),
            assumed_master_state: None,
        }],
    )
    .await
    .unwrap();
    let (id,): (u64,) = sqlx::query_as("SELECT id FROM shopping_items WHERE ulid = ?")
        .bind(ULID)
        .fetch_one(&pool)
        .await
        .unwrap();

    let edits = {
        let pool = pool.clone();
        tokio::spawn(async move {
            for n in 0..60 {
                let (rev,): (u64,) = sqlx::query_as("SELECT rev FROM shopping_items WHERE id = ?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await?;
                // A stale rev is a conflict, which is fine; an error is not.
                let assumed = ShoppingDoc { rev, ..row("Milk") };
                sync_repo::push_shopping(
                    &pool,
                    user,
                    vec![PushEntry {
                        new_document_state: row(&format!("Milk {n}")),
                        assumed_master_state: Some(assumed),
                    }],
                )
                .await?;
            }
            anyhow::Ok(())
        })
    };
    let trash = {
        let pool = pool.clone();
        tokio::spawn(async move {
            for _ in 0..60 {
                shopping_repo::delete(&pool, user, id).await?;
                shopping_repo::restore(&pool, user, ULID).await?;
            }
            anyhow::Ok(())
        })
    };
    edits.await.expect("task").expect("no push may fail");
    trash.await.expect("task").expect("no restore may fail");
}
