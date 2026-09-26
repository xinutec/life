//! Nextcloud Login Flow v2 → a non-expiring app password for CalDAV, which the
//! identity OAuth2 token can't reach. The user grants access at the `login` URL
//! and we poll until NC returns `{ server, loginName, appPassword }`.
//!
//! <https://docs.nextcloud.com/server/latest/developer_manual/client_apis/LoginFlow/>

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use serde::Deserialize;

pub struct LoginFlowInit {
    /// URL the user opens to grant access.
    pub login_url: String,
    /// Endpoint to poll for completion.
    pub poll_endpoint: String,
    /// Token identifying this flow at the poll endpoint.
    pub poll_token: String,
}

#[derive(Deserialize)]
struct InitiateResponse {
    poll: Poll,
    login: String,
}
#[derive(Deserialize)]
struct Poll {
    token: String,
    endpoint: String,
}

/// Step 1: `POST {base}/index.php/login/v2`.
pub async fn initiate(http: &reqwest::Client, base_url: &str) -> Result<LoginFlowInit> {
    let res = http
        .post(format!("{base_url}/index.php/login/v2"))
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow!("login-flow initiate failed: {}", res.status()));
    }
    let parsed: InitiateResponse = res.json().await.context("parsing login-flow initiate")?;
    Ok(LoginFlowInit {
        login_url: parsed.login,
        poll_endpoint: parsed.poll.endpoint,
        poll_token: parsed.poll.token,
    })
}

pub struct AppPassword {
    pub login_name: String,
    pub app_password: String,
}

#[derive(Deserialize)]
struct PollResponse {
    #[serde(rename = "loginName")]
    login_name: String,
    #[serde(rename = "appPassword")]
    app_password: String,
}

/// Step 3, one iteration. `Ok(None)` = not granted yet (NC returns 404);
/// `Ok(Some(_))` = granted. The caller drives the retry loop + deadline.
pub async fn poll_once(
    http: &reqwest::Client,
    init: &LoginFlowInit,
) -> Result<Option<AppPassword>> {
    let res = http
        .post(&init.poll_endpoint)
        .form(&[("token", init.poll_token.as_str())])
        .send()
        .await?;
    match res.status().as_u16() {
        200 => {
            let p: PollResponse = res.json().await.context("parsing login-flow poll")?;
            Ok(Some(AppPassword {
                login_name: p.login_name,
                app_password: p.app_password,
            }))
        }
        404 => Ok(None),
        s => Err(anyhow!("login-flow poll: unexpected status {s}")),
    }
}

/// `Basic base64(loginName:appPassword)` for CalDAV requests.
pub fn basic_auth_header(login_name: &str, app_password: &str) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{login_name}:{app_password}"));
    format!("Basic {encoded}")
}
