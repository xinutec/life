//! Our own sessions: Nextcloud is touched only at login. Cookie:
//! `<id>.<hex hmac_sha256(id)>`, verified in constant time.

use anyhow::Result;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use chrono::{Duration, NaiveDateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use sha2::Sha256;
use sqlx::MySqlPool;

use crate::error::AppError;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// An idle timeout, not a cap: the expiry slides on every request.
const SESSION_TTL_DAYS: i64 = 7;

/// The expiry is rewritten only when it would move by more than this.
const RENEW_INTERVAL_HOURS: i64 = 1;

pub const COOKIE_NAME: &str = "session";

#[derive(Clone, Debug)]
pub struct UserSession {
    pub user_id: String,
    pub display_name: String,
}

/// Kept apart, as a bare 401 hides four stories; only expiry is routine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionReject {
    NoCookie,
    BadSignature,
    Unknown,
    Expired,
}

impl SessionReject {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoCookie => "no_cookie",
            Self::BadSignature => "bad_signature",
            Self::Unknown => "unknown_session",
            Self::Expired => "expired",
        }
    }
}

#[derive(Clone, Debug)]
pub enum SessionLookup {
    Valid(UserSession),
    Rejected(SessionReject),
}

pub fn sign_value(secret: &str, value: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(value.as_bytes());
    format!("{value}.{}", hex::encode(mac.finalize().into_bytes()))
}

/// `verify_slice` is constant-time.
pub fn verify_value(secret: &str, signed: &str) -> Option<String> {
    let idx = signed.rfind('.')?;
    let (value, dotted_sig) = signed.split_at(idx);
    let sig = hex::decode(&dotted_sig[1..]).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(value.as_bytes());
    mac.verify_slice(&sig).ok()?;
    Some(value.to_string())
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: String,
    display_name: String,
    expires_at: NaiveDateTime,
}

pub async fn create_session(pool: &MySqlPool, secret: &str, user: &UserSession) -> Result<String> {
    let mut id_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut id_bytes);
    let id = hex::encode(id_bytes);
    let expires = (Utc::now() + Duration::days(SESSION_TTL_DAYS)).naive_utc();
    sqlx::query("INSERT INTO sessions (id, user_id, display_name, expires_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&user.user_id)
        .bind(&user.display_name)
        .bind(expires)
        .execute(pool)
        .await?;
    tracing::info!(user = %user.user_id, %expires, "session created");
    Ok(sign_value(secret, &id))
}

/// Slides the expiry, and lazily deletes an expired row. The row is the only clock.
pub async fn resolve_session(
    pool: &MySqlPool,
    secret: &str,
    signed: &str,
) -> Result<SessionLookup> {
    let Some(id) = verify_value(secret, signed) else {
        return Ok(SessionLookup::Rejected(SessionReject::BadSignature));
    };
    let row: Option<SessionRow> =
        sqlx::query_as("SELECT user_id, display_name, expires_at FROM sessions WHERE id = ?")
            .bind(&id)
            .fetch_optional(pool)
            .await?;
    let Some(row) = row else {
        return Ok(SessionLookup::Rejected(SessionReject::Unknown));
    };
    let now = Utc::now().naive_utc();
    if row.expires_at < now {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(&id)
            .execute(pool)
            .await?;
        return Ok(SessionLookup::Rejected(SessionReject::Expired));
    }

    let fresh = now + Duration::days(SESSION_TTL_DAYS);
    if fresh - row.expires_at > Duration::hours(RENEW_INTERVAL_HOURS) {
        sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
            .bind(fresh)
            .bind(&id)
            .execute(pool)
            .await?;
    }

    Ok(SessionLookup::Valid(UserSession {
        user_id: row.user_id,
        display_name: row.display_name,
    }))
}

pub async fn destroy_session(pool: &MySqlPool, secret: &str, signed: &str) -> Result<()> {
    if let Some(id) = verify_value(secret, signed) {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(&id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Rejects with 401 without a valid session cookie.
pub struct AuthUser(pub UserSession);

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(cookie) = jar.get(COOKIE_NAME) else {
            tracing::info!(
                reason = SessionReject::NoCookie.as_str(),
                "session rejected"
            );
            return Err(AppError::Unauthorized);
        };
        match resolve_session(&app.pool, &app.cfg.session_secret, cookie.value()).await? {
            SessionLookup::Valid(user) => Ok(AuthUser(user)),
            SessionLookup::Rejected(reason) => {
                tracing::info!(reason = reason.as_str(), "session rejected");
                Err(AppError::Unauthorized)
            }
        }
    }
}

/// Lazy expiry never reaches an abandoned cookie's row. Sessions are not user data.
pub async fn sweep_expired(pool: &MySqlPool) -> Result<u64> {
    let res = sqlx::query("DELETE FROM sessions WHERE expires_at < NOW()")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
