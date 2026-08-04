//! The raw-document archive (product_documents, 0034): keep every fetched source
//! payload verbatim, one per (product, source, kind), so we never fetch it twice.
//! Runs only when LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId};
use life::products::repo;
use life::products::source::Source;

#[tokio::test]
async fn stores_a_payload_verbatim_overwrites_by_kind_and_cascades() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode: Barcode = "5000000000901".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        Source::Asda,
        &"doc-cin-1".parse::<ExternalId>().unwrap(),
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Doc Test Product"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Nothing stored yet.
    assert!(
        repo::get_document(&pool, product.id, Source::Asda, "page")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repo::documents_for(&pool, product.id)
            .await
            .unwrap()
            .is_empty()
    );

    // Store a page blob verbatim.
    let blob =
        r#"{"calculatedNutrition":[{"nameValue":"Energy (kcal)","per100":61}],"vegan":true}"#;
    repo::upsert_document(&pool, product.id, Source::Asda, "page", blob)
        .await
        .unwrap();

    assert_eq!(
        repo::get_document(&pool, product.id, Source::Asda, "page")
            .await
            .unwrap()
            .as_deref(),
        Some(blob),
        "the body comes back byte-for-byte"
    );
    let docs = repo::documents_for(&pool, product.id).await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].source, Source::Asda);
    assert_eq!(docs[0].kind, "page");
    assert_eq!(
        usize::try_from(docs[0].bytes).expect("a byte count fits a usize"),
        blob.len(),
        "size hint = payload length"
    );
    assert!(docs[0].fetched_at > 0, "carries a fetch time");

    // Re-fetching the same kind overwrites (last fetch wins), still one row.
    let blob2 = r#"{"calculatedNutrition":[],"vegan":false}"#;
    repo::upsert_document(&pool, product.id, Source::Asda, "page", blob2)
        .await
        .unwrap();
    assert_eq!(
        repo::get_document(&pool, product.id, Source::Asda, "page")
            .await
            .unwrap()
            .as_deref(),
        Some(blob2)
    );
    assert_eq!(
        repo::documents_for(&pool, product.id).await.unwrap().len(),
        1,
        "same (source,kind) overwrites, doesn't accumulate"
    );

    // A different kind coexists.
    repo::upsert_document(&pool, product.id, Source::Off, "product", "{}")
        .await
        .unwrap();
    assert_eq!(
        repo::documents_for(&pool, product.id).await.unwrap().len(),
        2
    );

    // Deleting the product cascades the documents.
    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM product_documents WHERE product_id = ?")
            .bind(product.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "documents cascade on product delete");
}
