//! The shop-listing cache against a real MariaDB: remembering what a shop query
//! showed us, and answering "does this shop carry this barcode?" from memory.
//! Runs only when LIFE_TEST_DATABASE_URL is set.

use life::db;
use life::products::shop_cache::{self, CachedListing};
use sqlx::MySqlPool;

fn listing(source: &str, external_id: &str, barcode: Option<&str>) -> CachedListing {
    CachedListing {
        source: source.to_string(),
        external_id: external_id.to_string(),
        barcode: barcode.map(str::to_string),
        name: Some("Natural Yoghurt".to_string()),
        brand: Some("Yeo Valley".to_string()),
        quantity_label: Some("950G".to_string()),
        image_url: Some("https://example.test/y.jpg".to_string()),
    }
}

async fn fresh_pool() -> Option<MySqlPool> {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping shop cache DB test");
        return None;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    sqlx::query("DELETE FROM shop_listings WHERE external_id LIKE 'test-%'")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

#[tokio::test]
async fn every_hit_from_one_search_is_remembered_not_just_the_match() {
    let Some(pool) = fresh_pool().await else {
        return;
    };

    // The whole point: a search returns many hits, each carrying its own EAN.
    // All of them are stored, so a later lookup for ANY of these barcodes is
    // answered without asking the shop again.
    let hits = vec![
        listing("asda", "test-1", Some("5000000000001")),
        listing("asda", "test-2", Some("5000000000002")),
        listing("asda", "test-3", Some("5000000000003")),
    ];
    shop_cache::remember(&pool, &hits).await.unwrap();

    for (external_id, barcode) in [
        ("test-1", "5000000000001"),
        ("test-2", "5000000000002"),
        ("test-3", "5000000000003"),
    ] {
        let found = shop_cache::find_by_barcode(&pool, "asda", barcode)
            .await
            .unwrap()
            .expect("a remembered listing");
        assert_eq!(found.external_id, external_id);
        assert_eq!(found.barcode.as_deref(), Some(barcode));
    }
}

#[tokio::test]
async fn an_unknown_barcode_is_a_dont_know_not_a_no() {
    let Some(pool) = fresh_pool().await else {
        return;
    };
    // Nothing cached for this barcode → None. Callers must read this as "ask the
    // shop", never as "the shop doesn't carry it".
    let found = shop_cache::find_by_barcode(&pool, "asda", "9999999999999")
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn a_thinner_sighting_never_erases_what_we_already_learned() {
    let Some(pool) = fresh_pool().await else {
        return;
    };

    // A Waitrose product fetch taught us the barcode...
    shop_cache::remember(
        &pool,
        &[listing("waitrose", "test-w1", Some("5000000000010"))],
    )
    .await
    .unwrap();

    // ...then a later Waitrose *search* re-sees the same line number with no
    // barcode (search hits don't carry one). It must not blank the EAN — this
    // is the silent-erasure shape that bit product_dietary_flags in inc 6.
    let thin = CachedListing {
        barcode: None,
        brand: None,
        quantity_label: None,
        image_url: None,
        ..listing("waitrose", "test-w1", None)
    };
    shop_cache::remember(&pool, &[thin]).await.unwrap();

    let found = shop_cache::find_by_barcode(&pool, "waitrose", "5000000000010")
        .await
        .unwrap()
        .expect("the barcode survives a thinner re-sighting");
    assert_eq!(found.external_id, "test-w1");
    assert_eq!(found.brand.as_deref(), Some("Yeo Valley"));
}

#[tokio::test]
async fn re_seeing_a_listing_updates_its_description() {
    let Some(pool) = fresh_pool().await else {
        return;
    };
    shop_cache::remember(&pool, &[listing("asda", "test-r1", Some("5000000000020"))])
        .await
        .unwrap();

    let renamed = CachedListing {
        name: Some("Natural Bio Live Yoghurt".to_string()),
        ..listing("asda", "test-r1", Some("5000000000020"))
    };
    shop_cache::remember(&pool, &[renamed]).await.unwrap();

    let found = shop_cache::find_by_barcode(&pool, "asda", "5000000000020")
        .await
        .unwrap()
        .expect("still one row");
    assert_eq!(found.name.as_deref(), Some("Natural Bio Live Yoghurt"));

    // Upsert, not insert: the shop's identity is the key, so re-seeing a listing
    // must not duplicate it.
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM shop_listings WHERE source='asda' AND external_id=?")
            .bind("test-r1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn shops_keep_their_own_memories() {
    let Some(pool) = fresh_pool().await else {
        return;
    };
    // Same barcode, two shops: each is its own listing, and a lookup is always
    // scoped to the shop being asked about.
    shop_cache::remember(
        &pool,
        &[
            listing("asda", "test-s1", Some("5000000000030")),
            listing("waitrose", "test-s2", Some("5000000000030")),
        ],
    )
    .await
    .unwrap();

    let a = shop_cache::find_by_barcode(&pool, "asda", "5000000000030")
        .await
        .unwrap()
        .expect("asda");
    let w = shop_cache::find_by_barcode(&pool, "waitrose", "5000000000030")
        .await
        .unwrap()
        .expect("waitrose");
    assert_eq!(a.external_id, "test-s1");
    assert_eq!(w.external_id, "test-s2");
}
