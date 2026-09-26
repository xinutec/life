//! life — personal home OS backend. Entry point: load config, connect the DB,
//! run migrations, serve. All logic lives in the `life` library crate.

use anyhow::Result;
use life::wellbeing::{suggest, suggest_store};
use life::{config::Config, db, routes, state::AppState, sync};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env()?;
    if cfg.dev_login_user.is_some() {
        tracing::warn!(
            "DEV_LOGIN_USER is set — /dev-login mints sessions without Nextcloud. \
             NEVER set this in production."
        );
    }
    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;
    sync::backfill(&pool).await?;

    // Reap abandoned sessions hourly (the first tick fires immediately, so
    // boot also sweeps). Expiry is otherwise only enforced lazily, when the
    // same cookie is presented again.
    let sweep_pool = pool.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match life::session::sweep_expired(&sweep_pool).await {
                Ok(n) if n > 0 => tracing::info!("swept {n} expired session(s)"),
                Ok(_) => {}
                Err(e) => tracing::warn!("session sweep failed: {e:#}"),
            }
        }
    });

    // Bound every outbound call (Nextcloud identity/login-flow, Open Food Facts
    // metadata) so a hung upstream can't tie up the pod.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let bind_addr = cfg.bind_addr.clone();
    let state = AppState::new(pool, cfg, http);

    // Rebuild the day's emotion prompt just after the UTC rollover. The few-shot
    // ends at yesterday, so the prompt and its KV-cache prefix change at midnight;
    // prefilling it on a timer means no check-in waits for it. Best-effort: with
    // no stored vocabulary it skips the night and the keystroke warm-up covers it.
    let warm_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(suggest::until_rollover_warm(chrono::Utc::now())).await;
            match warm_the_day(&warm_state).await {
                Ok(true) => tracing::info!("queued the rollover preload for today's prompt"),
                Ok(false) => tracing::debug!("no remembered vocabulary — skipping the preload"),
                Err(e) => tracing::warn!("rollover preload failed: {e:#}"),
            }
        }
    });

    let stop_state = state.clone();
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("life listening on {bind_addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            stop_state.begin_shutdown();
        })
        .await?;
    tracing::info!("life stopped");
    Ok(())
}

/// Resolves on SIGTERM or ctrl-c, so `serve` finishes the requests in flight.
///
/// A handler that fails to install is logged and never fires, rather than
/// panicking: losing clean shutdown is no reason to refuse to run.
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let sigterm = async {
        match signal(SignalKind::terminate()) {
            // `recv()` yields None only if the handler is dropped, which it
            // isn't here; treat it as "never" either way.
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!("cannot listen for SIGTERM, shutdown will not be graceful: {e:#}");
                std::future::pending::<()>().await;
            }
        }
    };
    let sigint = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("cannot listen for ctrl-c: {e:#}");
            std::future::pending::<()>().await;
        }
    };

    tokio::select! {
        () = sigterm => tracing::info!("SIGTERM — draining in-flight requests"),
        () = sigint => tracing::info!("interrupted — draining in-flight requests"),
    }
}

/// Queue a preload of today's emotion prompt, built from the vocabulary the
/// picker last offered. `false` when there is none to build from.
async fn warm_the_day(app: &AppState) -> anyhow::Result<bool> {
    let Some((user_id, candidates)) = suggest_store::latest_vocabulary(&app.pool).await? else {
        return Ok(false);
    };
    let examples = suggest::fetch_examples(&app.pool, &user_id, suggest::MAX_EXAMPLES).await?;
    app.request_warm(suggest::build_system(&candidates, &examples));
    Ok(true)
}
