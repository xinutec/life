//! A login in progress, in a signed cookie bound to the browser that started it.
//! Needed because, from a browser with no Nextcloud session, `oauth2/authorize`
//! detours through Login Flow and the callback arrives with an empty `state=`.
//! `state` is still checked when returned. Accepted risk: a login CSRF needs
//! someone on the VPN to land a callback within the 10-minute window.

use chrono::{DateTime, Duration, Utc};
use rand::Rng;

use crate::session::{sign_value, verify_value};

pub const COOKIE_NAME: &str = "oauth_pending";

pub fn ttl() -> Duration {
    Duration::seconds(600)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingLogin {
    /// Sent as `state`; compared when Nextcloud returns it.
    pub nonce: String,
    /// Allowlist-checked when used.
    pub return_to: Option<String>,
    pub expires_at: DateTime<Utc>,
}

impl PendingLogin {
    /// `<expiry unix>|<nonce>|<return_to>`: `return_to` last, as it may hold `|`.
    fn encode(&self) -> String {
        format!(
            "{}|{}|{}",
            self.expires_at.timestamp(),
            self.nonce,
            self.return_to.as_deref().unwrap_or_default()
        )
    }

    fn decode(raw: &str) -> Option<Self> {
        let mut parts = raw.splitn(3, '|');
        let expires_at = DateTime::from_timestamp(parts.next()?.parse().ok()?, 0)?;
        let nonce = parts.next()?.to_string();
        let return_to = match parts.next()? {
            "" => None,
            p => Some(p.to_string()),
        };
        Some(Self {
            nonce,
            return_to,
            expires_at,
        })
    }
}

/// A fresh nonce and the signed cookie that remembers it.
pub fn issue(secret: &str, return_to: Option<String>, now: DateTime<Utc>) -> (String, String) {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    let pending = PendingLogin {
        nonce: hex::encode(bytes),
        return_to,
        expires_at: now + ttl(),
    };
    (pending.nonce.clone(), sign_value(secret, &pending.encode()))
}

/// The pending login if this callback belongs to it. An empty `state` (lost in
/// Login Flow) leaves the cookie to stand alone; a present one must match.
pub fn accept(
    secret: &str,
    cookie: Option<&str>,
    state: Option<&str>,
    now: DateTime<Utc>,
) -> Option<PendingLogin> {
    let pending = PendingLogin::decode(&verify_value(secret, cookie?)?)?;
    if pending.expires_at < now {
        return None;
    }
    match state {
        Some(s) if !s.is_empty() && s != pending.nonce => None,
        _ => Some(pending),
    }
}
