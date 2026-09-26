//! Routes against a real MariaDB, signed in through a real session. Import's
//! picture URL never resolves, so nothing here needs the network; which
//! pictures an import fetches is pinned in `picture_reconcile.rs`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use life::config::Config;
use life::db;
use life::products::ids::Barcode;
use life::products::repo;
use life::products::source::Source;
use life::routes;
use life::session::{COOKIE_NAME, UserSession, create_session};
use life::state::AppState;
use sqlx::MySqlPool;
use tower::ServiceExt;

const SECRET: &str = "test-secret";
/// On Waitrose's allowlisted CDN, but under a name that never resolves.
const UNREACHABLE_PICTURE: &str = "https://nowhere.invalid.wtrecom.com/LN_1_BP_11.jpg";

async fn pool() -> MySqlPool {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

fn state(pool: MySqlPool) -> AppState {
    let url = common::test_db_url();
    let cfg = Config {
        database_url: url,
        session_secret: SECRET.into(),
        bind_addr: "127.0.0.1:0".into(),
        nc_base_url: "https://nc.example".into(),
        nc_client_id: "id".into(),
        nc_client_secret: "secret".into(),
        nc_redirect_uri: "https://life.example/auth/callback".into(),
        static_dir: None,
        dev_login_user: None,
        house_scene: "scenes/house.json".into(),
        bins_ical_url: None,
        emotion_worker_token: None,
    };
    AppState::new(pool, cfg, reqwest::Client::new())
}

async fn signed_in(pool: &MySqlPool) -> String {
    let user = UserSession {
        user_id: "products-http-test".into(),
        display_name: "Products Test".into(),
    };
    let cookie = create_session(pool, SECRET, &user).await.expect("session");
    format!("{COOKIE_NAME}={cookie}")
}

async fn import(pool: &MySqlPool, barcode: &str, external_id: &str) -> StatusCode {
    let body = serde_json::json!({
        "source": "waitrose",
        "external_id": external_id,
        "name": "Essential Basmati Rice",
        "brand": "Waitrose Ltd",
        "barcode": barcode,
        "image_url": UNREACHABLE_PICTURE,
    });
    let req = Request::post("/api/products/import")
        .header(header::COOKIE, signed_in(pool).await)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = routes::router(state(pool.clone()))
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let _ = res.into_body().collect().await;
    status
}

async fn fresh(pool: &MySqlPool, barcode: &Barcode) {
    sqlx::query(
        "DELETE l FROM product_listings l JOIN products p ON p.id = l.product_id \
         WHERE p.barcode = ?",
    )
    .bind(barcode)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn attaching_a_shop_keeps_the_picture_the_product_already_has() {
    let pool = pool().await;
    let bc: Barcode = "9990000000957".parse().unwrap();
    fresh(&pool, &bc).await;
    repo::upsert(
        &pool,
        &bc,
        Some("Basmati rice"),
        None,
        None,
        Some((vec![1, 2, 3], "image/jpeg".into())),
    )
    .await
    .unwrap();
    let before = repo::get(&pool, &bc).await.unwrap().unwrap();
    repo::set_image_provenance(&pool, before.id, Source::Off)
        .await
        .unwrap();

    assert_eq!(
        import(&pool, "9990000000957", "900957").await,
        StatusCode::OK
    );

    let after = repo::get_by_id(&pool, before.id).await.unwrap().unwrap();
    let (bytes, _) = repo::get_image_by_id(&pool, before.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, vec![1, 2, 3], "the held picture is not replaced");
    assert_eq!(after.image_source, Some(Source::Off));

    // The shop's picture is offered instead, as a choice.
    let listings = repo::listings_for(&pool, before.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, before.id).await.unwrap();
    let offered = repo::picture_divergence(&after, &listings, &decisions)
        .expect("the shop's picture is offered for reconcile");
    assert!(
        offered
            .candidates
            .iter()
            .any(|c| c.source == Source::Waitrose && c.value == UNREACHABLE_PICTURE)
    );
}

#[tokio::test]
async fn an_unreachable_picture_does_not_fail_the_import() {
    // The listing is stored before the picture is fetched, so failing the
    // request would report an error for a change that was made.
    let pool = pool().await;
    let bc: Barcode = "9990000000964".parse().unwrap();
    fresh(&pool, &bc).await;

    assert_eq!(
        import(&pool, "9990000000964", "900964").await,
        StatusCode::OK
    );

    let product = repo::get(&pool, &bc).await.unwrap().expect("imported");
    assert!(!product.has_image);
    let listings = repo::listings_for(&pool, product.id).await.unwrap();
    assert!(listings.iter().any(|l| l.source == Source::Waitrose));
}

/// GET a product image, optionally revalidating with `If-None-Match`.
async fn image(
    pool: &MySqlPool,
    id: u64,
    etag: Option<&str>,
) -> (StatusCode, Option<String>, String) {
    let mut req = Request::get(format!("/api/products/id/{id}/image"))
        .header(header::COOKIE, signed_in(pool).await);
    if let Some(tag) = etag {
        req = req.header(header::IF_NONE_MATCH, tag);
    }
    let res = routes::router(state(pool.clone()))
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let get = |h| {
        res.headers()
            .get(h)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let (etag, cache) = (
        get(header::ETAG),
        get(header::CACHE_CONTROL).unwrap_or_default(),
    );
    (res.status(), etag, cache)
}

#[tokio::test]
async fn a_changed_picture_is_served_at_once_and_an_unchanged_one_is_a_304() {
    // The URL stays the same when the server replaces a picture (an import, a
    // reconcile, another device), so a day-long cache kept the old one.
    let pool = pool().await;
    let bc: Barcode = "9990000000971".parse().unwrap();
    fresh(&pool, &bc).await;
    repo::upsert(
        &pool,
        &bc,
        Some("Rice"),
        None,
        None,
        Some((vec![1, 2, 3], "image/jpeg".into())),
    )
    .await
    .unwrap();
    let id = repo::get(&pool, &bc).await.unwrap().unwrap().id;

    let (status, etag, cache) = image(&pool, id.0, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        cache.contains("no-cache"),
        "revalidated on every view: {cache}"
    );
    let etag = etag.expect("an ETag to revalidate with");

    let (status, _, _) = image(&pool, id.0, Some(&etag)).await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);

    repo::set_image_by_id(&pool, id, &[4, 5, 6], "image/jpeg")
        .await
        .unwrap();
    let (status, new_tag, _) = image(&pool, id.0, Some(&etag)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the new picture, not the cached one"
    );
    assert_ne!(new_tag.as_deref(), Some(etag.as_str()));
}

const SIGNED_IN_USER: &str = "products-http-test";

async fn item_for(pool: &MySqlPool, user: &str, name: &str) -> u64 {
    sqlx::query("INSERT INTO items (user_id, name, category) VALUES (?, ?, 'appliance')")
        .bind(user)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert item")
        .last_insert_id()
}

async fn purchase_on(pool: &MySqlPool, user: &str, item: u64, name: &str) -> u64 {
    let bought = life::purchases::repo::BoughtItem {
        id: item,
        product_id: None,
        barcode: None,
        name,
        quantity: None,
        unit: None,
    };
    let paid = life::purchases::types::NewPurchase {
        shop: "Argos".into(),
        amount_minor: 2999,
        currency: "GBP".into(),
        bought_on: None,
        warranty_months: None,
    };
    life::purchases::repo::record(pool, user, &bought, &paid)
        .await
        .expect("record purchase")
}

async fn attach(pool: &MySqlPool, item: u64, purchase: u64) -> StatusCode {
    let req = Request::post(format!("/api/items/{item}/files"))
        .header(header::COOKIE, signed_in(pool).await)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header("x-file-name", "receipt.pdf")
        .header("x-purchase-id", purchase.to_string())
        .body(Body::from(&b"%PDF-1.4\n%receipt"[..]))
        .unwrap();
    let res = routes::router(state(pool.clone()))
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let _ = res.into_body().collect().await;
    status
}

#[tokio::test]
async fn a_receipt_can_only_prove_a_live_purchase_of_its_own_item() {
    // The purchase id arrives in a header, so without a check a file could be
    // tied to another user's purchase, another item's, or one in the trash.
    let pool = pool().await;
    let other = "products-http-test-other";
    for user in [SIGNED_IN_USER, other] {
        sqlx::query("DELETE FROM purchases WHERE user_id = ?")
            .bind(user)
            .execute(&pool)
            .await
            .expect("clean");
    }
    let kettle = item_for(&pool, SIGNED_IN_USER, "Kettle").await;
    let toaster = item_for(&pool, SIGNED_IN_USER, "Toaster").await;
    let theirs = item_for(&pool, other, "Their kettle").await;
    let own = purchase_on(&pool, SIGNED_IN_USER, kettle, "Kettle").await;
    let toasters = purchase_on(&pool, SIGNED_IN_USER, toaster, "Toaster").await;
    let foreign = purchase_on(&pool, other, theirs, "Their kettle").await;
    let trashed = purchase_on(&pool, SIGNED_IN_USER, kettle, "Kettle").await;
    assert!(
        life::purchases::repo::remove(&pool, SIGNED_IN_USER, kettle, trashed)
            .await
            .expect("remove")
    );

    assert_eq!(attach(&pool, kettle, own).await, StatusCode::OK);
    assert_eq!(
        attach(&pool, kettle, toasters).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        attach(&pool, kettle, foreign).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        attach(&pool, kettle, trashed).await,
        StatusCode::BAD_REQUEST
    );
}
