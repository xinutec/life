//! Whether the picker is told a worker is listening — the in-memory liveness
//! judgement behind `SuggestEmotionsResponse.pending`.
//!
//! No DB and no worker: the pool is lazy and never connects, because none of
//! this touches a query. The whole question is what the pod believes about a
//! machine it cannot dial. (`#[tokio::test]` only because sqlx 0.9's lazy pool
//! wants a runtime to exist — nothing here awaits.)

use life::config::Config;
use life::state::AppState;
use sqlx::mysql::MySqlPoolOptions;

fn state() -> AppState {
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
        static_dir: None,
        dev_login_user: None,
        house_scene: "scenes/house.json".into(),
        emotion_worker_token: None,
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
    // THE REGRESSION (2026-07-25). The worker is single-threaded: while it
    // preloads it does not poll, and the first check-in of a UTC day makes that
    // silence ~130s — a cold model load plus rebuilding the day's prefix cache.
    // Judged on polling alone the pod called it dead and the picker reported
    // that nothing was coming, while the worker was preparing for that exact
    // request. Handing out the preload is what we know instead.
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
