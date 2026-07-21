//! Router-level tests: drive `routes::router()` end-to-end via oneshot, no live
//! DB or socket. These cover the seams the repo/pure-fn tests can't reach — the
//! `AuthUser` 401 path, the `AppError`→status/JSON mapping, and the SPA/404
//! fallback. The pool is created lazily and never connects, because every path
//! here is rejected (401/404) before any query runs.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use life::config::Config;
use life::routes;
use life::state::AppState;
use sqlx::mysql::MySqlPoolOptions;
use tower::ServiceExt; // oneshot

fn test_state() -> AppState {
    state_with_static(None)
}

/// `static_dir` = Some mounts the SPA fallback, as production does; None is
/// API-only. The pool is lazy: constructing it does not connect (only used on
/// paths we never reach here). The URL just has to parse.
fn state_with_static(static_dir: Option<String>) -> AppState {
    let pool = MySqlPoolOptions::new()
        .connect_lazy("mysql://life:life@127.0.0.1:3307/life")
        .expect("lazy pool");
    let cfg = Config {
        database_url: "mysql://life:life@127.0.0.1:3307/life".into(),
        session_secret: "test-secret".into(),
        bind_addr: "127.0.0.1:0".into(),
        nc_base_url: "https://nc.example".into(),
        nc_client_id: "id".into(),
        nc_client_secret: "secret".into(),
        nc_redirect_uri: "https://life.example/auth/callback".into(),
        static_dir,
        dev_login_user: None,
        house_scene: "scenes/house.json".into(),
        emotion_worker_token: None,
    };
    let http = reqwest::Client::new();
    AppState::new(pool, cfg, http)
}

async fn get(path: &str) -> (StatusCode, String) {
    send(Request::get(path).body(Body::empty()).unwrap()).await
}

async fn send(req: Request<Body>) -> (StatusCode, String) {
    let (status, body, _) = send_to(test_state(), req).await;
    (status, body)
}

/// Drive a request against a given state; also hands back the Content-Type,
/// which is the whole point when asserting "this is an API answer, not a page".
async fn send_to(state: AppState, req: Request<Body>) -> (StatusCode, String, String) {
    let res = routes::router(state).oneshot(req).await.unwrap();
    let status = res.status();
    let content_type = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes).into_owned(),
        content_type,
    )
}

#[tokio::test]
async fn healthz_is_open() {
    let (status, body) = get("/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn protected_api_requires_auth_and_maps_to_401_json() {
    // A representative sample of the authenticated surface — no cookie present.
    for path in [
        "/api/me",
        "/api/items",
        "/api/todo",
        "/api/trash",
        "/api/conflicts",
    ] {
        let (status, body) = get(path).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "GET {path}");
        // AppError::Unauthorized renders as a JSON error body, not empty/plain.
        assert!(
            body.contains("\"error\"") && body.contains("not authenticated"),
            "GET {path} body was {body:?}"
        );
    }
}

#[tokio::test]
async fn mutations_also_require_auth() {
    let req = axum::http::Request::post("/api/todo")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let (status, _) = send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn telemetry_is_not_an_open_log_write() {
    // The client activity trace lands in the server log, so an UNauthenticated
    // caller must not be able to POST to it — otherwise it's an anonymous
    // log-injection endpoint. A valid batch shape, no cookie → 401, not 204.
    let req = axum::http::Request::post("/api/telemetry")
        .header("content-type", "application/json")
        .body(Body::from(r#"[{"kind":"nav","path":"/today","at":1}]"#))
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body.contains("\"error\"") && body.contains("not authenticated"),
        "body was {body:?}"
    );
}

#[tokio::test]
async fn dev_login_is_absent_without_dev_login_user() {
    // The route is only mounted when DEV_LOGIN_USER is set; here it isn't, so
    // it falls through to 404 (no static_dir either).
    let (status, _) = get("/dev-login").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_path_is_404_when_api_only() {
    let (status, _) = get("/no/such/thing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A throwaway SPA bundle — just the index.html the fallback serves. Returned
/// path is removed by the caller.
fn spa_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("life-routes-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create spa dir");
    std::fs::write(dir.join("index.html"), "<!doctype html><title>Life</title>")
        .expect("write index.html");
    dir
}

#[tokio::test]
async fn unknown_api_path_is_a_json_404_even_with_the_spa_mounted() {
    // In production STATIC_DIR is set, so a catch-all serves index.html for
    // client-side routes. It must NOT catch /api/*: answering an API call with
    // 200 text/html is exactly what the client reads as a lapsed session (see
    // the frontend's classifyFetchResponse — "a 2xx body that isn't JSON"), so
    // a retired or mistyped route would surface to the user as a bogus
    // "signed out" instead of an honest 404.
    let dir = spa_dir("api404");
    let state = state_with_static(Some(dir.to_string_lossy().into_owned()));
    let req = Request::get("/api/products/id/1/no-such-endpoint") // a route that doesn't exist
        .body(Body::empty())
        .unwrap();
    let (status, body, content_type) = send_to(state, req).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "body was {body:?}");
    assert!(
        content_type.starts_with("application/json"),
        "an API 404 must answer in JSON, got {content_type:?} / {body:?}"
    );
    assert!(body.contains("\"error\""), "body was {body:?}");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn an_unknown_non_api_path_still_falls_back_to_the_spa() {
    // The other half of the contract: client-side routes (/product/42, /today…)
    // are NOT server routes, and must still boot the app rather than 404.
    let dir = spa_dir("spa");
    let state = state_with_static(Some(dir.to_string_lossy().into_owned()));
    let req = Request::get("/product/42").body(Body::empty()).unwrap();
    let (status, body, content_type) = send_to(state, req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type.starts_with("text/html"),
        "got {content_type:?}"
    );
    assert!(body.contains("<title>Life</title>"), "body was {body:?}");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn conflict_restore_bad_kind_is_400_after_auth() {
    // Unauthenticated first — proves the ordering — then the point stands that
    // an unknown trash kind maps to 400 via BadRequest (exercised in the DB
    // test for the authed path). Here we assert the auth gate wins.
    let req = axum::http::Request::post("/api/trash/bogus/1/restore")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
