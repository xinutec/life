//! A whole sign-in through the real router, against a stand-in Nextcloud that
//! grants any code: the cookies it leaves behind.

mod test_config;

mod common;

use axum::Json;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use life::config::Config;
use life::state::AppState;
use life::{db, pending_login, routes, session};
use tower::ServiceExt;

/// A Nextcloud that trades any code for a token naming `user`.
async fn nextcloud() -> String {
    let nc = axum::Router::new()
        .route(
            "/index.php/apps/oauth2/api/v1/token",
            post(|| async { Json(serde_json::json!({ "access_token": "token" })) }),
        )
        .route(
            "/ocs/v2.php/cloud/user",
            get(|| async {
                Json(serde_json::json!({ "ocs": { "data": { "id": "user", "displayname": "User" } } }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, nc).await.unwrap() });
    format!("http://{addr}")
}

async fn send(app: &axum::Router, req: Request<Body>) -> Response {
    app.clone().oneshot(req).await.unwrap()
}

fn set_cookies(res: &Response) -> Vec<String> {
    res.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

/// The `Set-Cookie` line for `name`, which a test expects to find.
fn cookie_line<'a>(lines: &'a [String], name: &str) -> &'a str {
    lines
        .iter()
        .find(|l| l.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no Set-Cookie for {name}: {lines:?}"))
}

/// `name=value`, as a browser sends it back.
fn pair(line: &str) -> String {
    line.split(';').next().unwrap().to_owned()
}

fn assert_removed_at_root(lines: &[String], name: &str) {
    let dropped = cookie_line(lines, name);
    assert!(dropped.contains("Max-Age=0"), "{dropped}");
    assert!(
        dropped.contains("Path=/"),
        "removed where it was set: {dropped}"
    );
}

/// A cookie set at `Path=/` is removed only by a removal at `Path=/`. Without
/// one, the browser scopes the removal to the request's directory — `/auth`
/// for the callback — and the cookie it meant to drop stays.
#[tokio::test]
async fn signing_in_and_out_removes_each_cookie_where_it_was_set() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let cfg = Config {
        nc_base_url: nextcloud().await,
        ..test_config::config()
    };
    let app = routes::router(AppState::new(pool, cfg, reqwest::Client::new()));

    let login = send(&app, Request::get("/login").body(Body::empty()).unwrap()).await;
    let pending = pair(cookie_line(
        &set_cookies(&login),
        pending_login::COOKIE_NAME,
    ));
    let to_nc = login.headers()[header::LOCATION].to_str().unwrap();
    let state = url::Url::parse(to_nc)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("the authorize URL carries a state");

    let back = send(
        &app,
        Request::get(format!("/auth/callback?code=granted&state={state}"))
            .header(header::COOKIE, &pending)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(back.status(), StatusCode::SEE_OTHER);
    let lines = set_cookies(&back);
    assert_removed_at_root(&lines, pending_login::COOKIE_NAME);
    let signed_in = pair(cookie_line(&lines, session::COOKIE_NAME));

    let out = send(
        &app,
        Request::post("/logout")
            .header(header::COOKIE, &signed_in)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(out.status(), StatusCode::NO_CONTENT);
    assert_removed_at_root(&set_cookies(&out), session::COOKIE_NAME);
}
