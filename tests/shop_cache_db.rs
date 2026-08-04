//! The shop-listing cache against a real MariaDB: remembering what a shop query
//! showed us, and answering "does this shop carry this barcode?" from memory.
//! Runs only when LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::Barcode;
use life::products::shop_cache::{self, CachedListing};
use life::products::source::Source;
use sqlx::MySqlPool;

fn listing(source: Source, external_id: &str, barcode: Option<&str>) -> CachedListing {
    CachedListing {
        source,
        external_id: external_id.parse().unwrap(),
        barcode: barcode.map(|b| b.parse().unwrap()),
        name: Some("Natural Yoghurt".to_string()),
        brand: Some("Yeo Valley".to_string()),
        quantity_label: Some("950G".to_string()),
        image_url: Some("https://example.test/y.jpg".to_string()),
    }
}

async fn fresh_pool() -> MySqlPool {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    sqlx::query("DELETE FROM shop_listings WHERE external_id LIKE 'test-%'")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn every_hit_from_one_search_is_remembered_not_just_the_match() {
    let pool = fresh_pool().await;

    // The whole point: a search returns many hits, each carrying its own EAN.
    // All of them are stored, so a later lookup for ANY of these barcodes is
    // answered without asking the shop again.
    let hits = vec![
        listing(Source::Asda, "test-1", Some("5000000000001")),
        listing(Source::Asda, "test-2", Some("5000000000002")),
        listing(Source::Asda, "test-3", Some("5000000000003")),
    ];
    shop_cache::remember(&pool, &hits).await.unwrap();

    for (external_id, barcode) in [
        ("test-1", "5000000000001"),
        ("test-2", "5000000000002"),
        ("test-3", "5000000000003"),
    ] {
        let found = shop_cache::find_by_barcode(&pool, Source::Asda, &barcode.parse().unwrap())
            .await
            .unwrap()
            .expect("a remembered listing");
        assert_eq!(found.external_id, external_id);
        assert_eq!(found.barcode.as_ref().map(Barcode::as_str), Some(barcode));
    }
}

#[tokio::test]
async fn an_unknown_barcode_is_a_dont_know_not_a_no() {
    let pool = fresh_pool().await;
    // Nothing cached for this barcode → None. Callers must read this as "ask the
    // shop", never as "the shop doesn't carry it".
    let found = shop_cache::find_by_barcode(
        &pool,
        Source::Asda,
        &"9999999999999".parse::<Barcode>().unwrap(),
    )
    .await
    .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn a_thinner_sighting_never_erases_what_we_already_learned() {
    let pool = fresh_pool().await;

    // A Waitrose product fetch taught us the barcode...
    shop_cache::remember(
        &pool,
        &[listing(Source::Waitrose, "test-w1", Some("5000000000010"))],
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
        ..listing(Source::Waitrose, "test-w1", None)
    };
    shop_cache::remember(&pool, &[thin]).await.unwrap();

    let found = shop_cache::find_by_barcode(
        &pool,
        Source::Waitrose,
        &"5000000000010".parse::<Barcode>().unwrap(),
    )
    .await
    .unwrap()
    .expect("the barcode survives a thinner re-sighting");
    assert_eq!(found.external_id, "test-w1");
    assert_eq!(found.brand.as_deref(), Some("Yeo Valley"));
}

#[tokio::test]
async fn re_seeing_a_listing_updates_its_description() {
    let pool = fresh_pool().await;
    shop_cache::remember(
        &pool,
        &[listing(Source::Asda, "test-r1", Some("5000000000020"))],
    )
    .await
    .unwrap();

    let renamed = CachedListing {
        name: Some("Natural Bio Live Yoghurt".to_string()),
        ..listing(Source::Asda, "test-r1", Some("5000000000020"))
    };
    shop_cache::remember(&pool, &[renamed]).await.unwrap();

    let found = shop_cache::find_by_barcode(
        &pool,
        Source::Asda,
        &"5000000000020".parse::<Barcode>().unwrap(),
    )
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
    let pool = fresh_pool().await;
    // Same barcode, two shops: each is its own listing, and a lookup is always
    // scoped to the shop being asked about.
    shop_cache::remember(
        &pool,
        &[
            listing(Source::Asda, "test-s1", Some("5000000000030")),
            listing(Source::Waitrose, "test-s2", Some("5000000000030")),
        ],
    )
    .await
    .unwrap();

    let a = shop_cache::find_by_barcode(
        &pool,
        Source::Asda,
        &"5000000000030".parse::<Barcode>().unwrap(),
    )
    .await
    .unwrap()
    .expect("asda");
    let w = shop_cache::find_by_barcode(
        &pool,
        Source::Waitrose,
        &"5000000000030".parse::<Barcode>().unwrap(),
    )
    .await
    .unwrap()
    .expect("waitrose");
    assert_eq!(a.external_id, "test-s1");
    assert_eq!(w.external_id, "test-s2");
}
