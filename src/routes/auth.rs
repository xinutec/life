//! Nextcloud login, and the app-password link for the calendar.

use std::time::{Duration, Instant};

use anyhow::anyhow;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::AppError;
use crate::nextcloud::credentials::{self, LinkStatus};
use crate::nextcloud::{identity, login_flow};
use crate::pending_login;
use crate::session::{AuthUser, COOKIE_NAME, UserSession, create_session, destroy_session};
use crate::state::AppState;

/// Outlives the session on purpose: the `sessions` row is the only clock and it
/// slides on every request. 400 days is the browsers' ceiling.
fn session_cookie(value: String) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(400))
        .build()
}

/// `Lax`: the callback is a top-level navigation from Nextcloud.
fn pending_cookie(value: String) -> Cookie<'static> {
    Cookie::build((pending_login::COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(pending_login::ttl().num_seconds()))
        .build()
}

/// Same-site paths only. Rejects `//host` and `/\host`, which browsers fold
/// to `//`.
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

/// GET /login → redirect to Nextcloud's authorize endpoint, remembering the login
/// in a signed cookie (see [`pending_login`]).
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

/// The OAuth callback: where Nextcloud sends the browser. A failure is drawn as
/// a page, because `AppError`'s JSON in its place reads as the app being broken.
pub async fn callback(
    State(app): State<AppState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    match finish_sign_in(app, jar, q).await {
        Ok(done) => done.into_response(),
        Err(e) => sign_in_problem(e.into_response().status()),
    }
}

/// A sign-in that could not be finished, said in words and with a way back in.
fn sign_in_problem(status: StatusCode) -> Response {
    let said = match status {
        StatusCode::UNAUTHORIZED => "This sign-in did not start here, or it took too long.",
        StatusCode::FORBIDDEN => "This account may not use this app.",
        _ => "The sign-in could not be finished.",
    };
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Sign-in did not finish</title><style>\
         body{{font:16px/1.5 system-ui,-apple-system,sans-serif;margin:0;\
         min-height:100vh;display:grid;place-items:center;padding:1.5rem;color:#1a1a1a}}\
         main{{max-width:26rem}}h1{{font-size:1.2rem;margin:0 0 .5rem}}\
         p{{margin:0 0 1.5rem;color:#555}}\
         a{{display:inline-block;padding:.65rem 1.1rem;border-radius:.5rem;\
         background:#1b6ac9;color:#fff;text-decoration:none}}\
         </style></head><body><main><h1>Sign-in did not finish</h1>\
         <p>{said}</p><a href=\"/login\">Try again</a></main></body></html>"
    );
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// The callback's work: exchange the code and create our session. Every failure
/// is logged.
async fn finish_sign_in(
    app: AppState,
    jar: CookieJar,
    q: CallbackQuery,
) -> Result<(CookieJar, Redirect), AppError> {
    let Some(pending) = pending_login::accept(
        &app.cfg.session_secret,
        jar.get(pending_login::COOKIE_NAME).map(Cookie::value),
        q.state.as_deref(),
        Utc::now(),
    ) else {
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
    // Dropped, so it cannot be replayed.
    let jar = jar.remove(Cookie::from(pending_login::COOKIE_NAME));
    Ok((jar.add(session_cookie(signed)), Redirect::to(&dest)))
}

pub async fn logout(
    State(app): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(c) = jar.get(COOKIE_NAME) {
        destroy_session(&app.pool, &app.cfg.session_secret, c.value()).await?;
    }
    tracing::info!("logged out");
    Ok((
        jar.remove(Cookie::from(COOKIE_NAME)),
        StatusCode::NO_CONTENT,
    ))
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConnectStarted {
    pub login_url: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConnectState {
    pub status: LinkStatus,
}

/// POST /api/nextcloud/connect/init → start Login Flow v2 and poll for the grant
/// in the background.
pub async fn connect_init(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<ConnectStarted>, AppError> {
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

    Ok(Json(ConnectStarted { login_url }))
}

/// GET /dev-login → development only: a session for `DEV_LOGIN_USER`. Mounted
/// only when that is set, and re-checked here.
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
) -> Result<Json<ConnectState>, AppError> {
    let status = credentials::status(&app.pool, &user.user_id).await?;
    Ok(Json(ConnectState { status }))
}
