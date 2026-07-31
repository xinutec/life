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

    // Rebuild the day's emotion prompt just after the UTC rollover.
    //
    // The few-shot is cut off at the end of yesterday, so the system prompt — and
    // the model's KV-cache prefix keyed on it — changes at midnight. Someone has
    // to pay the ~50s of prefill that rebuilds it, and until now it was whoever
    // wrote the day's first check-in, on top of the ~60s cold model load, while
    // they waited. Doing it on a timer means nobody is waiting: the preload is
    // queued into the same warm slot a keystroke would use, and the worker picks
    // it up on its next poll.
    //
    // Best-effort throughout. No stored vocabulary (a fresh database, or a user
    // who has not opened the picker since) simply skips the night — the keystroke
    // warm-up still covers the load, exactly as it did before.
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

    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("life listening on {bind_addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("life stopped");
    Ok(())
}

/// Resolves when the platform asks us to stop, so `serve` can finish the
/// requests already in flight instead of dropping them.
///
/// Without this the process had no SIGTERM handler at all, so the signal took
/// its default action and killed it outright: every `kubectl rollout restart`
/// cut whatever was mid-request, and the pod exited 143 — which Kubernetes
/// reports as `Error`, the kind of routine red that teaches you to ignore red.
///
/// A failure to install a handler is logged and that arm simply never fires,
/// rather than panicking: losing the *ability* to shut down cleanly is not a
/// reason to refuse to run.
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
