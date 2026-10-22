//! Open Food Facts lookups against a stand-in server: "not there" and "could
//! not ask" are different answers.

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;

use life::products::ids::Barcode;
use life::products::off::fetch_from;

async fn product(Path(file): Path<String>) -> axum::response::Response {
    match file.as_str() {
        "12345670.json" => (StatusCode::NOT_FOUND, r#"{"status":0}"#).into_response(),
        "23456781.json" => (StatusCode::SERVICE_UNAVAILABLE, "down").into_response(),
        "34567892.json" => (
            StatusCode::OK,
            r#"{"status":1,"product":{"product_name":"Oat drink"}}"#,
        )
            .into_response(),
        _ => StatusCode::IM_A_TEAPOT.into_response(),
    }
}

async fn stand_in() -> String {
    let app = Router::new().route("/api/v2/product/{file}", get(product));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn code(s: &str) -> Barcode {
    s.parse().unwrap()
}

#[tokio::test]
async fn a_product_off_does_not_know_is_none() {
    let base = stand_in().await;
    let got = fetch_from(&reqwest::Client::new(), &base, &code("12345670")).await;
    assert!(got.expect("a 404 is an answer").is_none());
}

#[tokio::test]
async fn an_outage_is_an_error_not_a_missing_product() {
    let base = stand_in().await;
    let got = fetch_from(&reqwest::Client::new(), &base, &code("23456781")).await;
    assert!(
        got.is_err(),
        "a 503 must not read as 'not in Open Food Facts'"
    );
}

#[tokio::test]
async fn a_known_product_is_returned() {
    let base = stand_in().await;
    let got = fetch_from(&reqwest::Client::new(), &base, &code("34567892"))
        .await
        .expect("ok")
        .expect("found");
    assert_eq!(got.name.as_deref(), Some("Oat drink"));
}

/// A redirect from an allowed host could point at an internal one, so the image
/// client hands the 3xx back instead of following it.
#[tokio::test]
async fn the_image_client_does_not_follow_redirects() {
    let app = Router::new()
        .route(
            "/img.jpg",
            get(|| async { axum::response::Redirect::temporary("/internal") }),
        )
        .route("/internal", get(|| async { "followed" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = life::products::off::image_client().expect("client");
    let res = client
        .get(format!("http://{addr}/img.jpg"))
        .send()
        .await
        .expect("an answer");
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
}
