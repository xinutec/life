//! Auth routes: NC identity login + the NC app-password (CalDAV) link flow.

use std::time::{Duration, Instant};

use anyhow::anyhow;
use axum::Json;
use axum::extract::{Query, State};
use axum::response::Redirect;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::AppError;
use crate::nextcloud::{credentials, identity, login_flow};
use crate::pending_login;
use crate::session::{AuthUser, COOKIE_NAME, UserSession, create_session, destroy_session};
use crate::state::AppState;

/// The cookie deliberately outlives the session it names: the `sessions` row is
/// the only clock, and it slides forward on every request (see
/// `session::resolve_session`). A cookie that expired with the row would sign you
/// out 7 days after logging in no matter how much you used the app — the bug this
/// pairs with. Presenting a cookie whose row is gone is simply a 401, so the long
/// life costs nothing. 400 days is the browser's own ceiling.
fn session_cookie(value: String) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(400))
        .build()
}

/// The login-in-progress cookie. `Lax`, because the callback arrives as a
/// top-level navigation from Nextcloud and a `Strict` cookie would not be sent.
fn pending_cookie(value: String) -> Cookie<'static> {
    Cookie::build((pending_login::COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(pending_login::ttl().num_seconds()))
        .build()
}

/// Only allow same-site internal paths as a post-login redirect target.
/// Rejects `//host` (protocol-relative) and `/\host` — browsers fold `\` to
/// `/` in special-scheme URLs, so a Location of `/\evil.com` would redirect
/// off-site.
pub fn validate_return_to(return_to: Option<&str>) -> String {
    match return_to {
        Some(p) if p.starts_with('/') && !p[1..].starts_with(['/', '\\']) => p.to_string(),
        _ => "/".to_string(),
    }
}

#[derive(Deserialize)]
pub struct LoginQuery {
    return_to: Option<String>,
}

/// GET /login → redirect to NC's OAuth2 authorize endpoint, remembering the login
/// in a signed cookie (see [`pending_login`] for why the `state` echo isn't enough).
pub async fn login(
    State(app): State<AppState>,
    jar: CookieJar,
    Query(q): Query<LoginQuery>,
) -> (CookieJar, Redirect) {
    tracing::info!(return_to = ?q.return_to, "login started");
    let (nonce, cookie) = pending_login::issue(&app.cfg.session_secret, q.return_to, Utc::now());
    (
        jar.add(pending_cookie(cookie)),
        Redirect::to(&identity::authorize_url(&app.cfg, &nonce)),
    )
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

/// GET /auth/callback → exchange code, read identity, create our session.
///
/// Every way this can fail says so in the log. A login that dies here otherwise
/// looks like a bare 401 and tells you nothing.
pub async fn callback(
    State(app): State<AppState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Result<(CookieJar, Redirect), AppError> {
    let Some(pending) = pending_login::accept(
        &app.cfg.session_secret,
        jar.get(pending_login::COOKIE_NAME).map(Cookie::value),
        q.state.as_deref(),
        Utc::now(),
    ) else {
        // No cookie (the login didn't start here), older than 10 minutes at the NC
        // consent screen, or a `state` that doesn't match the one we minted.
        tracing::warn!(reason = "no_pending_login", "login callback rejected");
        return Err(AppError::Unauthorized);
    };
    let code = q.code.ok_or_else(|| {
        tracing::warn!(reason = "no_code", "login callback rejected");
        anyhow!("missing authorization code")
    })?;

    let token = identity::exchange_code(&app.http, &app.cfg, &code).await?;
    let nc_user = identity::fetch_user(&app.http, &app.cfg, &token).await?;

    let user = UserSession {
        user_id: nc_user.id,
        display_name: nc_user.display_name,
    };
    let signed = create_session(&app.pool, &app.cfg.session_secret, &user).await?;
    let dest = validate_return_to(pending.return_to.as_deref());
    tracing::info!(user = %user.user_id, %dest, "login complete");
    // The login is over: drop its cookie so a stale one can't be replayed.
    let jar = jar.remove(Cookie::from(pending_login::COOKIE_NAME));
    Ok((jar.add(session_cookie(signed)), Redirect::to(&dest)))
}

/// POST /logout → destroy the session + clear the cookie.
pub async fn logout(
    State(app): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    if let Some(c) = jar.get(COOKIE_NAME) {
        destroy_session(&app.pool, &app.cfg.session_secret, c.value()).await?;
    }
    tracing::info!("logged out");
    Ok((jar.remove(Cookie::from(COOKIE_NAME)), Redirect::to("/")))
}

/// POST /api/nextcloud/connect/init → start Login Flow v2, return the grant
/// URL, and poll for completion in the background until granted or timeout.
pub async fn connect_init(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, AppError> {
    let init = login_flow::initiate(&app.http, &app.cfg.nc_base_url).await?;
    let login_url = init.login_url.clone();

    let http = app.http.clone();
    let pool = app.pool.clone();
    let user_id = user.user_id.clone();
    tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if Instant::now() > deadline {
                tracing::warn!("login-flow timed out for {user_id}");
                break;
            }
            match login_flow::poll_once(&http, &init).await {
                Ok(Some(creds)) => {
                    match credentials::store(&pool, &user_id, &creds).await {
                        Ok(()) => tracing::info!("nc app password linked for {user_id}"),
                        Err(e) => tracing::error!("storing nc creds: {e:#}"),
                    }
                    break;
                }
                Ok(None) => tokio::time::sleep(Duration::from_secs(2)).await,
                Err(e) => {
                    tracing::error!("login-flow poll failed: {e:#}");
                    break;
                }
            }
        }
    });

    Ok(Json(json!({ "login_url": login_url })))
}

/// GET /dev-login → DEV ONLY. Mints a session for `DEV_LOGIN_USER` with no
/// Nextcloud. The route is only mounted when that env var is set (see
/// routes::router); this handler also re-checks, so it 404s otherwise.
pub async fn dev_login(
    State(app): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    let user_id = app.cfg.dev_login_user.clone().ok_or(AppError::NotFound)?;
    let user = UserSession {
        display_name: user_id.clone(),
        user_id,
    };
    let signed = create_session(&app.pool, &app.cfg.session_secret, &user).await?;
    Ok((jar.add(session_cookie(signed)), Redirect::to("/")))
}

/// GET /api/nextcloud/connect/status → active | needs_reauth | not_linked.
pub async fn connect_status(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, AppError> {
    let status = credentials::status(&app.pool, &user.user_id).await?;
    Ok(Json(json!({ "status": status })))
}
