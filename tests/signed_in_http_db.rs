//! Routes against a real MariaDB, signed in through a real session. Import's
//! picture URL never resolves, so nothing here needs the network; which
//! pictures an import fetches is pinned in `picture_reconcile.rs`.

mod test_config;

mod catalogue;
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
        ..test_config::config()
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
    catalogue::looked_up(
        &pool,
        &bc,
        Some("Basmati rice"),
        None,
        None,
        Some((vec![1, 2, 3], "image/jpeg".into())),
    )
    .await;
    let before = repo::get(&pool, &bc).await.unwrap().unwrap();
    sqlx::query("UPDATE products SET image_source = 'off' WHERE id = ?")
        .bind(before.id)
        .execute(&pool)
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
    // reconcile, another device), so a cache that never revalidates keeps the old one.
    let pool = pool().await;
    let bc: Barcode = "9990000000971".parse().unwrap();
    fresh(&pool, &bc).await;
    catalogue::looked_up(
        &pool,
        &bc,
        Some("Rice"),
        None,
        None,
        Some((vec![1, 2, 3], "image/jpeg".into())),
    )
    .await;
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

async fn item_for(pool: &MySqlPool, user: &str, name: &str) -> life::inventory::types::ItemId {
    sqlx::query("INSERT INTO items (user_id, name, category) VALUES (?, ?, 'appliance')")
        .bind(user)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert item")
        .last_insert_id()
        .into()
}

async fn purchase_on(
    pool: &MySqlPool,
    user: &str,
    item: life::inventory::types::ItemId,
    name: &str,
) -> life::purchases::types::PurchaseId {
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
        currency: life::products::prices::Currency::gbp(),
        bought_on: None,
        warranty_months: None,
    };
    life::purchases::repo::record(pool, user, &bought, &paid)
        .await
        .expect("record purchase")
}

async fn attach(
    pool: &MySqlPool,
    item: life::inventory::types::ItemId,
    purchase: life::purchases::types::PurchaseId,
) -> StatusCode {
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
    // No wipe: the tests in this file share a user and run in parallel, and every
    // purchase here hangs off an item made just now.
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

/// A signed-in request with an optional JSON body: the status and the parsed
/// answer (`Null` when there is none).
async fn call(
    pool: &MySqlPool,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, signed_in(pool).await);
    if body.is_some() {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    let res = routes::router(state(pool.clone()))
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let answer = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the API answers in JSON")
    };
    (status, answer)
}

/// A catalogue row for `barcode`, starting from nothing.
async fn catalogued(pool: &MySqlPool, barcode: &str, name: &str) -> u64 {
    let bc: Barcode = barcode.parse().unwrap();
    fresh(pool, &bc).await;
    catalogue::looked_up(pool, &bc, Some(name), None, None, None).await;
    repo::get(pool, &bc).await.unwrap().unwrap().id.0
}

#[tokio::test]
async fn a_shop_page_is_only_stored_against_its_own_product() {
    // The WebView posts a page it fetched; its EAN is what makes the facts this
    // product's. Another product's page must not land here.
    let pool = pool().await;
    let id = catalogued(&pool, "9990000000988", "Oat drink").await;
    let page = include_str!("fixtures/asda_brandbank_oalty.json");
    let post =
        |ean: &str, source: &str| serde_json::json!({ "source": source, "ean": ean, "blob": page });

    let (status, _) = call(
        &pool,
        "POST",
        &format!("/api/products/id/{id}/facts"),
        Some(post("9990000000995", "asda")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "another product's page");
    let (status, _) = call(
        &pool,
        "POST",
        &format!("/api/products/id/{id}/facts"),
        Some(post("9990000000988", "waitrose")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no parser for that shop");
    let (_, detail) = call(&pool, "GET", &format!("/api/products/id/{id}"), None).await;
    assert!(detail["facts"]["nutrition"].is_null(), "nothing was stored");
    assert_eq!(detail["documents"], serde_json::json!([]));

    let (status, detail) = call(
        &pool,
        "POST",
        &format!("/api/products/id/{id}/facts"),
        Some(post("9990000000988", "asda")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["facts"]["nutrition"]["energy_kcal"], 61.0);
    assert_eq!(detail["documents"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn what_the_phone_saw_at_a_shop_answers_the_next_question() {
    // Waitrose is behind a bot-wall, so the phone looks and reports back. The
    // next "does Waitrose carry it?" must come from that report, and before any
    // report the answer is "not looked", never "no".
    let pool = pool().await;
    let id = catalogued(&pool, "9990000001008", "Basmati").await;
    sqlx::query(
        "DELETE FROM shop_listings WHERE source = 'waitrose' AND external_id = 'T-LN-1008'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let find = format!("/api/products/id/{id}/find/waitrose");

    let (status, before) = call(&pool, "GET", &find, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(before["searched"], false);
    assert!(before["hit"].is_null());

    let seen = serde_json::json!([
        { "external_id": "T-LN-1008", "barcode": "9990000001008", "name": "Essential Basmati" },
    ]);
    let (status, stored) = call(
        &pool,
        "POST",
        "/api/products/shop/waitrose/listings",
        Some(seen),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored["remembered"], 1);

    let (_, after) = call(&pool, "GET", &find, None).await;
    assert_eq!(after["from_cache"], true);
    assert_eq!(after["hit"]["external_id"], "T-LN-1008");

    let (status, _) = call(
        &pool,
        "GET",
        &format!("/api/products/id/{id}/find/tesco"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "not a shop we know");
    let bad = serde_json::json!([{ "external_id": "T-LN-1", "barcode": "not-a-barcode" }]);
    let (status, _) = call(
        &pool,
        "POST",
        "/api/products/shop/waitrose/listings",
        Some(bad),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a bad barcode would poison the index"
    );
}

#[tokio::test]
async fn reconcile_refuses_what_it_cannot_honour_as_a_bad_request() {
    let pool = pool().await;
    let id = catalogued(&pool, "9990000001015", "Rice").await;
    let reconcile = format!("/api/products/id/{id}/reconcile");
    let choose =
        |field: &str, choice: &str| Some(serde_json::json!([{ "field": field, "choice": choice }]));

    for (field, choice, why) in [
        ("picture", "user", "a picture is uploaded, not typed"),
        ("picture", "asda", "no Asda listing offers a picture"),
        ("nutrition", "off", "no source has a panel"),
    ] {
        let (status, _) = call(&pool, "POST", &reconcile, choose(field, choice)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}={choice}: {why}");
    }
    let (status, detail) = call(&pool, "POST", &reconcile, choose("picture", "keep")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["product"]["id"], id);

    let (status, _) = call(
        &pool,
        "POST",
        "/api/products/id/999999999/reconcile",
        choose("picture", "keep"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_uploaded_picture_must_be_an_image_whatever_it_claims() {
    let pool = pool().await;
    catalogued(&pool, "9990000001022", "Lentils").await;
    let put = |content_type: &'static str, bytes: Vec<u8>| {
        let pool = pool.clone();
        async move {
            let req = Request::put("/api/products/9990000001022/image")
                .header(header::COOKIE, signed_in(&pool).await)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(bytes))
                .unwrap();
            routes::router(state(pool.clone()))
                .oneshot(req)
                .await
                .unwrap()
                .status()
        }
    };
    let png = [&b"\x89PNG\r\n\x1a\n"[..], &[0; 16]].concat();

    assert_eq!(
        put("text/plain", png.clone()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(put("image/png", vec![]).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        put("image/png", b"<svg onload=alert(1)>".to_vec()).await,
        StatusCode::BAD_REQUEST,
        "the bytes decide, not the header"
    );
    // A GIF labelled as a PNG is stored as what it is.
    let gif = [&b"GIF89a"[..], &[0; 16]].concat();
    assert_eq!(put("image/png", gif).await, StatusCode::NO_CONTENT);
    let bc: Barcode = "9990000001022".parse().unwrap();
    let (_, mime) = repo::get_image(&pool, &bc).await.unwrap().unwrap();
    assert_eq!(mime, "image/gif");
    assert_eq!(put("image/png", png).await, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_product_page_shows_my_purchases_and_nobody_elses() {
    let pool = pool().await;
    let other = "products-http-test-stranger";
    // Scoped to this test's barcode: a user-wide wipe would delete the
    // purchases of the tests running beside it.
    sqlx::query("DELETE FROM purchases WHERE barcode = '9990000001039'")
        .execute(&pool)
        .await
        .unwrap();
    let id = catalogued(&pool, "9990000001039", "Kettle").await;
    let paid = |amount_minor| life::purchases::types::NewPurchase {
        shop: "Argos".into(),
        amount_minor,
        currency: life::products::prices::Currency::gbp(),
        bought_on: None,
        warranty_months: None,
    };
    for (user, amount) in [(SIGNED_IN_USER, 2999), (other, 1500)] {
        let item = item_for(&pool, user, "Kettle").await;
        let bought = life::purchases::repo::BoughtItem {
            id: item,
            product_id: None,
            barcode: Some("9990000001039"),
            name: "Kettle",
            quantity: None,
            unit: None,
        };
        life::purchases::repo::record(&pool, user, &bought, &paid(amount))
            .await
            .unwrap();
    }

    let (status, detail) = call(&pool, "GET", &format!("/api/products/id/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let amounts: Vec<_> = detail["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["amount_minor"].clone())
        .collect();
    assert_eq!(
        amounts,
        [serde_json::json!(2999)],
        "found by barcode, and only mine"
    );

    let (status, _) = call(&pool, "GET", "/api/products/id/999999999", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_scanned_upc_finds_the_product_stored_under_its_ean() {
    // The scanner reads 12 digits; the catalogue holds the 13-digit form. The
    // lookup is answered from the catalogue, without asking Open Food Facts.
    let pool = pool().await;
    let id = catalogued(&pool, "0099900000017", "Peanut butter").await;

    let (status, product) = call(&pool, "GET", "/api/products/099900000017", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(product["id"], id);

    let (_, found) = call(&pool, "GET", "/api/products?q=peanut%20butt", None).await;
    assert!(
        found.as_array().unwrap().iter().any(|p| p["id"] == id),
        "the catalogue search finds it by name"
    );
    let (_, blank) = call(&pool, "GET", "/api/products?q=%20", None).await;
    assert_eq!(blank, serde_json::json!([]));
}

/// A stock row holding `quantity` of `unit`, for `user`.
async fn stock(
    pool: &MySqlPool,
    user: &str,
    name: &str,
    quantity: f64,
    unit: Option<&str>,
) -> life::inventory::types::ItemId {
    sqlx::query(
        "INSERT INTO items (user_id, name, category, quantity, unit) VALUES (?, ?, 'food', ?, ?)",
    )
    .bind(user)
    .bind(name)
    .bind(quantity)
    .bind(unit)
    .execute(pool)
    .await
    .expect("insert stock")
    .last_insert_id()
    .into()
}

#[tokio::test]
async fn using_stock_never_quietly_leaves_the_number_wrong() {
    // The cupboard's number must stay true: an amount it cannot be measured
    // against is refused out loud, naming the unit, rather than ignored.
    let pool = pool().await;
    let flour = stock(&pool, SIGNED_IN_USER, "Flour", 500.0, Some("g")).await;
    let eggs = item_for(&pool, SIGNED_IN_USER, "Eggs").await;
    let theirs = stock(
        &pool,
        "products-http-test-stranger",
        "Flour",
        500.0,
        Some("g"),
    )
    .await;
    let take = |id: life::inventory::types::ItemId, quantity: f64, unit: &str| {
        let pool = pool.clone();
        let body = serde_json::json!({ "quantity": quantity, "unit": unit });
        async move { call(&pool, "POST", &format!("/api/items/{id}/use"), Some(body)).await }
    };

    let (status, item) = take(flour, 200.0, "g").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(item["quantity"], 300.0);

    let (status, err) = take(flour, 1.0, "tbsp").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err["error"].as_str().unwrap().contains("measured in g"),
        "{err}"
    );
    let (status, _) = take(eggs, 1.0, "g").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a row with no quantity");
    let (status, _) = take(flour, 0.0, "g").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "nothing used");
    let (status, _) = take(theirs, 1.0, "g").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "someone else's cupboard");

    let (_, item) = take(flour, 400.0, "g").await;
    assert_eq!(item["quantity"], 0.0, "more than was there empties it");
}

#[tokio::test]
async fn someone_elses_item_is_a_dead_end() {
    // Neither a purchase filed against it nor its history: and the history
    // answers as for an unknown id, so it does not reveal that the id exists.
    let pool = pool().await;
    let theirs = item_for(&pool, "products-http-test-stranger", "Their fridge").await;
    let paid = serde_json::json!({ "shop": "Argos", "amount_minor": 100, "currency": "GBP" });

    let (status, _) = call(
        &pool,
        "POST",
        &format!("/api/items/{theirs}/purchases"),
        Some(paid.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, history) = call(&pool, "GET", &format!("/api/items/{theirs}/history"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        history,
        serde_json::json!({ "entries": [], "purchases": [] })
    );

    let mine = item_for(&pool, SIGNED_IN_USER, "My fridge").await;
    let (status, bought) = call(
        &pool,
        "POST",
        &format!("/api/items/{mine}/purchases"),
        Some(paid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bought["amount_minor"], 100);
    let bad = serde_json::json!({ "shop": "Argos", "amount_minor": -1, "currency": "GBP" });
    let (status, _) = call(
        &pool,
        "POST",
        &format!("/api/items/{mine}/purchases"),
        Some(bad),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a bad price is the request's fault"
    );
}

#[tokio::test]
async fn an_attachment_is_downloaded_never_rendered() {
    // Uploads are served from our own origin; rendering one would run it in the
    // app's security context. The file name is user text inside a header.
    let pool = pool().await;
    let item = item_for(&pool, SIGNED_IN_USER, "Boiler").await;
    let req = Request::post(format!("/api/items/{item}/files"))
        .header(header::COOKIE, signed_in(&pool).await)
        .header(header::CONTENT_TYPE, "image/png")
        .header("x-file-name", "manual\"; x=\"y.png")
        .body(Body::from([&b"\x89PNG\r\n\x1a\n"[..], &[0; 16]].concat()))
        .unwrap();
    let res = routes::router(state(pool.clone()))
        .oneshot(req)
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let file: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let req = Request::get(format!("/api/items/{item}/files/{}", file["id"]))
        .header(header::COOKIE, signed_in(&pool).await)
        .body(Body::empty())
        .unwrap();
    let res = routes::router(state(pool.clone()))
        .oneshot(req)
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let disposition = res.headers()[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert_eq!(disposition, "attachment; filename=\"manual; x=y.png\"");
}

/// Put a row on the Buy list the way the app does, by a sync push, and return
/// the id the buy route takes.
async fn on_the_list(pool: &MySqlPool, name: &str) -> u64 {
    let ulid = ulid::Ulid::new().to_string();
    let doc = serde_json::json!({
        "ulid": ulid, "name": name, "quantity": 2.0, "unit": "pots",
        "barcode": null, "category": "food", "done": false,
    });
    let push = serde_json::json!([{ "newDocumentState": doc }]);
    let (status, conflicts) = call(pool, "POST", "/api/sync/shopping", Some(push)).await;
    assert_eq!((status, conflicts), (StatusCode::OK, serde_json::json!([])));
    let (_, pulled) = call(pool, "GET", "/api/sync/shopping?since=0&limit=1000", None).await;
    pulled["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ulid"] == ulid.as_str())
        .and_then(|d| d["id"].as_u64())
        .expect("the pushed row is pulled with its id")
}

#[tokio::test]
async fn buying_a_row_brings_it_home_once_and_a_bad_price_never_stops_it() {
    let pool = pool().await;
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(SIGNED_IN_USER)
        .execute(&pool)
        .await
        .unwrap();
    // Named for this run: the user is shared with tests running beside this one.
    let run = ulid::Ulid::new();
    let yoghurt_name = format!("Yoghurt {run}");
    let yoghurt = on_the_list(&pool, &yoghurt_name).await;
    let paid = |amount_minor: i64| serde_json::json!({ "purchase": { "shop": "Waitrose", "amount_minor": amount_minor, "currency": "GBP" } });

    let (status, item) = call(
        &pool,
        "POST",
        &format!("/api/shopping/{}/buy", yoghurt),
        Some(paid(330)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(item["name"], yoghurt_name.as_str());
    assert_eq!(item["quantity"], 2.0, "the row's amount comes home with it");
    let item_id = life::inventory::types::ItemId(item["id"].as_u64().unwrap());
    let bought = life::purchases::repo::for_item(&pool, SIGNED_IN_USER, item_id)
        .await
        .unwrap();
    assert_eq!(bought.len(), 1);
    assert_eq!(bought[0].amount_minor, 330);
    assert!(
        life::shopping::repo::get(&pool, SIGNED_IN_USER, yoghurt)
            .await
            .unwrap()
            .is_none(),
        "it left the list"
    );

    // A second buy of the same row finds nothing, rather than a second item.
    let (status, _) = call(
        &pool,
        "POST",
        &format!("/api/shopping/{}/buy", yoghurt),
        Some(paid(330)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A price the server refuses is dropped, and the buy still stands.
    let milk = on_the_list(&pool, &format!("Milk {run}")).await;
    let (status, item) = call(
        &pool,
        "POST",
        &format!("/api/shopping/{}/buy", milk),
        Some(paid(-1)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item_id = life::inventory::types::ItemId(item["id"].as_u64().unwrap());
    assert!(
        life::purchases::repo::for_item(&pool, SIGNED_IN_USER, item_id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        life::inventory::repo::list_items(&pool, SIGNED_IN_USER)
            .await
            .unwrap()
            .iter()
            .filter(|i| i.name.ends_with(&run.to_string()))
            .count(),
        2,
        "two rows bought, two items, no duplicate"
    );
}

#[tokio::test]
async fn a_shop_import_fills_a_missing_pack_size_but_never_overrules_one() {
    // Open Food Facts' quantity is the product's own ("500g"); a shop's is the
    // pack it happens to sell ("22x27G"). The shop may fill the gap, not replace.
    let pool = pool().await;
    let import = |barcode: &str, external_id: &str| {
        serde_json::json!({
            "source": "waitrose",
            "external_id": external_id,
            "name": "Porridge Oats",
            "barcode": barcode,
            "quantity_label": "22x27G",
        })
    };

    let held = catalogued(&pool, "9990000001046", "Porridge Oats").await;
    sqlx::query("UPDATE products SET quantity_label = '500g' WHERE id = ?")
        .bind(held)
        .execute(&pool)
        .await
        .unwrap();
    let (status, product) = call(
        &pool,
        "POST",
        "/api/products/import",
        Some(import("9990000001046", "T-PACK-1")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        product["quantity_label"], "500g",
        "the product's own stands"
    );

    catalogued(&pool, "9990000001053", "Porridge Oats").await;
    let (_, product) = call(
        &pool,
        "POST",
        "/api/products/import",
        Some(import("9990000001053", "T-PACK-2")),
    )
    .await;
    assert_eq!(
        product["quantity_label"], "22x27G",
        "an empty one is filled"
    );
    assert_eq!(product["pack"]["value"], 594.0, "and read as an amount");
}

/// An unknown kind is a client bug; a known kind with nothing binned is just gone.
#[tokio::test]
async fn restoring_an_unknown_kind_is_a_400_and_an_unknown_ref_a_404() {
    let pool = pool().await;
    let (status, body) = call(&pool, "POST", "/api/trash/bogus/1/restore", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"].is_string(), "{body}");

    let (status, body) = call(&pool, "POST", "/api/trash/item/0/restore", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// A location id arrives from the client. One that is someone else's must not
/// become a place your things live, nor leak its name back through history.
#[tokio::test]
async fn somebody_elses_location_is_not_a_place_to_put_things() {
    let pool = pool().await;
    let other = "products-http-test-loc-other";
    let insert_loc = |user: &'static str, name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query("INSERT INTO locations (user_id, kind, name) VALUES (?, 'room', ?)")
                .bind(user)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_id()
        }
    };
    let theirs = insert_loc(other, "Their secret room").await;
    let mine = insert_loc(SIGNED_IN_USER, "My kitchen").await;
    let item = |loc: u64| serde_json::json!({ "name": "Jar", "location_id": loc });

    let (status, body) = call(&pool, "POST", "/api/items", Some(item(theirs))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "create: {body}");

    let (status, created) = call(&pool, "POST", "/api/items", Some(item(mine))).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_u64().unwrap();
    let (status, body) = call(
        &pool,
        "PATCH",
        &format!("/api/items/{id}"),
        Some(item(theirs)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "update: {body}");
    let move_to = serde_json::json!({ "location_id": theirs });
    let (status, body) = call(
        &pool,
        "POST",
        &format!("/api/items/{id}/move"),
        Some(move_to),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "move: {body}");
    let child = serde_json::json!({ "kind": "cupboard", "name": "Mine", "parent_id": theirs });
    let (status, body) = call(&pool, "POST", "/api/locations", Some(child)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "parent: {body}");

    // A history row pointing there (written before the guard) names nothing.
    sqlx::query(
        "INSERT INTO item_history (item_id, user_id, location_id, event) VALUES (?, ?, ?, 'moved')",
    )
    .bind(id)
    .bind(SIGNED_IN_USER)
    .bind(theirs)
    .execute(&pool)
    .await
    .unwrap();
    let (_, history) = call(&pool, "GET", &format!("/api/items/{id}/history"), None).await;
    assert!(
        !history.to_string().contains("Their secret room"),
        "{history}"
    );
    let (status, moved) = call(
        &pool,
        "POST",
        &format!("/api/items/{id}/move"),
        Some(serde_json::json!({ "location_id": mine })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "your own place still works: {moved}"
    );
}

/// A pushed doc whose enum the server does not know is refused as it is
/// decoded: a 400 in the API's own JSON, and nothing stored, so it can never be
/// written and then fail every later read of the list.
#[tokio::test]
async fn a_push_naming_an_unknown_kind_of_anything_is_refused_whole() {
    let pool = pool().await;
    let todo = |ulid: &str, kind: &str, status: &str, priority: Option<&str>| {
        serde_json::json!({ "newDocumentState": {
            "ulid": ulid, "title": "validate me", "type": kind, "status": status,
            "priority": priority, "notes": null,
        } })
    };
    let link = |ulid: &str, kind: &str, target: &str| {
        serde_json::json!({ "newDocumentState": {
            "ulid": ulid, "from": "01ISOVALFROMAAAAAAAAAAAAAA", "kind": kind,
            "targetKind": target, "targetRef": "x",
        } })
    };
    let shop = |ulid: &str, category: &str| {
        serde_json::json!({ "newDocumentState": {
            "ulid": ulid, "name": "validate me", "quantity": null, "unit": null,
            "barcode": null, "category": category, "done": false,
        } })
    };
    for (path, push, what) in [
        (
            "todo",
            vec![todo("01REFUSEDTODOA00000000000A", "task", "banana", None)],
            "status",
        ),
        (
            "todo",
            vec![todo("01REFUSEDTODOB00000000000B", "chore", "open", None)],
            "type",
        ),
        (
            "todo",
            vec![todo(
                "01REFUSEDTODOC00000000000C",
                "task",
                "open",
                Some("urgent"),
            )],
            "priority",
        ),
        // One good doc first: the whole batch is refused, not just the bad one.
        (
            "todo",
            vec![
                todo("01REFUSEDTODOD00000000000D", "task", "open", None),
                todo("01REFUSEDTODOE00000000000E", "task", "banana", None),
            ],
            "mixed batch",
        ),
        (
            "todo-link",
            vec![link("01REFUSEDLINKA00000000000A", "sibling", "todo")],
            "link kind",
        ),
        (
            "todo-link",
            vec![link("01REFUSEDLINKB00000000000B", "related", "person")],
            "target kind",
        ),
        (
            "shopping",
            vec![shop("01REFUSEDSHOPA00000000000A", "groceries")],
            "category",
        ),
    ] {
        let (status, body) = call(
            &pool,
            "POST",
            &format!("/api/sync/{path}"),
            Some(serde_json::Value::Array(push)),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
        assert!(body["error"].is_string(), "{what}: {body}");
    }
    for path in ["todo", "todo-link", "shopping"] {
        let (_, pulled) = call(
            &pool,
            "GET",
            &format!("/api/sync/{path}?since=0&limit=1000"),
            None,
        )
        .await;
        assert!(
            !pulled.to_string().contains("01REFUSED"),
            "nothing refused was stored: {pulled}"
        );
    }
}
