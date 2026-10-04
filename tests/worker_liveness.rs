//! Whether the picker is told a worker is listening — the in-memory liveness
//! judgement behind `SuggestEmotionsResponse.pending`.
//!
//! No DB and no worker: the pool is lazy and never connects, because none of
//! this touches a query. The whole question is what the pod believes about a
//! machine it cannot dial. (`#[tokio::test]` only because sqlx 0.9's lazy pool
//! wants a runtime to exist — nothing here awaits.)

mod test_config;

use life::config::Config;
use life::state::AppState;
use sqlx::mysql::MySqlPoolOptions;

fn state() -> AppState {
    state_with_token(None)
}

fn state_with_token(token: Option<&str>) -> AppState {
    let pool = MySqlPoolOptions::new()
        .connect_lazy("mysql://life:life@127.0.0.1:3307/life")
        .expect("lazy pool");
    let cfg = Config {
        emotion_worker_token: token.map(str::to_string),
        ..test_config::config()
    };
    AppState::new(pool, cfg, reqwest::Client::new())
}

#[tokio::test]
async fn a_pod_that_has_never_heard_from_a_worker_says_so() {
    // The picker must not promise an answer nobody is computing — including
    // right after a restart, before any worker has polled.
    assert!(!state().worker_alive());
}

#[tokio::test]
async fn a_worker_that_polled_is_alive() {
    let app = state();
    app.mark_worker_seen();
    assert!(app.worker_alive());
}

#[tokio::test]
async fn taking_a_preload_keeps_the_worker_alive_while_it_is_silent() {
    // The single-threaded worker does not poll while it preloads (~130s cold).
    // Judged on polling alone it would read as dead while preparing for this
    // very request; handing out the preload is the evidence instead.
    let app = state();
    app.request_warm("system prompt for today".into());
    assert_eq!(app.take_warm().as_deref(), Some("system prompt for today"));
    assert!(
        app.worker_alive(),
        "a worker that just took a preload is working, not gone"
    );
}

#[tokio::test]
async fn an_unconsumed_warm_request_proves_nothing() {
    // Queueing a preload says the APP wants one. Only a worker collecting it is
    // evidence that a worker exists — otherwise opening the picker on a machine
    // with no worker at all would claim one.
    let app = state();
    app.request_warm("system prompt".into());
    assert!(!app.worker_alive());
}

#[tokio::test]
async fn polling_again_ends_the_grace_rather_than_extending_it() {
    // Once it speaks the ordinary clock takes over, so the grace can never keep
    // a worker alive for longer than the one silence it was granted.
    let app = state();
    app.request_warm("system prompt".into());
    app.take_warm();
    app.mark_worker_seen();
    assert!(app.worker_alive());
}

#[tokio::test]
async fn one_preload_is_handed_out_once() {
    // The directive is consumed: a second poll during the same silence must not
    // be sent to redo the work (and re-arm the window off the back of it).
    let app = state();
    app.request_warm("system prompt".into());
    assert!(app.take_warm().is_some());
    assert!(app.take_warm().is_none());
}

/// A poll arriving while the pod stops answers empty at once, so the worker's
/// next poll reaches the pod replacing this one. No query runs on this path.
#[tokio::test]
async fn a_poll_during_shutdown_answers_empty_without_waiting() {
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use axum::response::IntoResponse;

    let app = state_with_token(Some("t"));
    app.begin_shutdown();
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer t"));
    let res = life::routes::emotion_worker::next(State(app), headers)
        .await
        .map(IntoResponse::into_response)
        .expect("a response");
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

/// Only the exact token is a worker: a near miss of any shape is refused. Asked
/// during shutdown, so an admitted poll answers at once without a query.
#[tokio::test]
async fn only_the_exact_token_is_a_worker() {
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use axum::response::IntoResponse;

    let app = state_with_token(Some("secret"));
    app.begin_shutdown();
    let poll = |auth: &'static str| {
        let app = app.clone();
        async move {
            let mut headers = HeaderMap::new();
            headers.insert(header::AUTHORIZATION, HeaderValue::from_static(auth));
            life::routes::emotion_worker::next(State(app), headers)
                .await
                .map_or_else(IntoResponse::into_response, IntoResponse::into_response)
                .status()
        }
    };
    for near_miss in [
        "Bearer secreT",
        "Bearer secre",
        "Bearer secrets",
        "Bearer ",
        "secret",
    ] {
        assert_eq!(
            poll(near_miss).await,
            StatusCode::UNAUTHORIZED,
            "{near_miss:?}"
        );
    }
    assert_eq!(poll("Bearer secret").await, StatusCode::NO_CONTENT);
    // With no token configured there is no worker channel at all.
    let closed = state_with_token(None);
    closed.begin_shutdown();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer secret"),
    );
    let res = life::routes::emotion_worker::next(State(closed), headers)
        .await
        .map_or_else(IntoResponse::into_response, IntoResponse::into_response);
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
