//! Configuration from the environment, read at startup.

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    /// `mysql://life:pw@host/life`.
    pub database_url: String,
    pub session_secret: String,
    pub bind_addr: String,

    /// No trailing slash.
    pub nc_base_url: String,
    /// For the login flow.
    pub nc_client_id: String,
    pub nc_client_secret: String,
    pub nc_redirect_uri: String,

    /// The Angular bundle; unset, the server is API-only.
    pub static_dir: Option<String>,

    /// Development only: `/dev-login` mints a session for this user. Never set in
    /// a deployment.
    pub dev_login_user: Option<String>,

    pub house_scene: String,

    /// The worker dials in, as the pod cannot dial the Mac. Unset: no suggestions.
    pub emotion_worker_token: Option<String>,

    /// e.g. `https://recyclingservices.brent.gov.uk/waste/<property>/calendar.ics`.
    /// Never a constant: the URL identifies one address. Unset: no bins.
    pub bins_ical_url: Option<String>,
}

fn env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let nc_base_url = env("NC_BASE_URL")?.trim_end_matches('/').to_string();
        // At boot rather than in the /login handler.
        let parsed = url::Url::parse(&nc_base_url)
            .with_context(|| format!("NC_BASE_URL is not a valid URL: {nc_base_url:?}"))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
            anyhow::bail!("NC_BASE_URL must be an http(s) URL with a host: {nc_base_url:?}");
        }
        Ok(Self {
            database_url: env("DATABASE_URL")?,
            session_secret: env("SESSION_SECRET")?,
            bind_addr: env_or("BIND_ADDR", "0.0.0.0:8080"),
            nc_base_url,
            nc_client_id: env("NC_CLIENT_ID")?,
            nc_client_secret: env("NC_CLIENT_SECRET")?,
            nc_redirect_uri: env("NC_REDIRECT_URI")?,
            static_dir: std::env::var("STATIC_DIR").ok(),
            dev_login_user: std::env::var("DEV_LOGIN_USER").ok(),
            house_scene: env_or("HOUSE_SCENE", "scenes/house.json"),
            emotion_worker_token: std::env::var("EMOTION_WORKER_TOKEN").ok(),
            bins_ical_url: std::env::var("BINS_ICAL_URL")
                .ok()
                .filter(|u| !u.trim().is_empty()),
        })
    }
}
