//! The Nextcloud app password: one per user, no expiry.

use anyhow::Result;
use sqlx::MySqlPool;
use ts_rs::TS;

use crate::nextcloud::login_flow::AppPassword;

#[derive(Debug, PartialEq, Eq, serde::Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "ConnectionStatus")]
pub enum LinkStatus {
    Active,
    NeedsReauth,
    NotLinked,
}

pub async fn store(pool: &MySqlPool, user_id: &str, creds: &AppPassword) -> Result<()> {
    sqlx::query(
        "INSERT INTO nc_credentials (user_id, login_name, app_password, status) \
         VALUES (?, ?, ?, 'active') \
         ON DUPLICATE KEY UPDATE login_name = VALUES(login_name), \
         app_password = VALUES(app_password), status = 'active'",
    )
    .bind(user_id)
    .bind(&creds.login_name)
    .bind(&creds.app_password)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct Credentials {
    pub login_name: String,
    pub app_password: String,
}

/// Not an `Option`: never linked and rejected ask different things of the user.
pub enum Usable {
    /// Login Flow v2 has never completed.
    NotLinked,
    /// Nextcloud rejected it once; it would fail the same way again.
    NeedsReauth,
    Ready(Credentials),
}

pub async fn for_dav(pool: &MySqlPool, user_id: &str) -> Result<Usable> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT login_name, app_password, status FROM nc_credentials WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        None => Usable::NotLinked,
        Some((_, _, status)) if status != "active" => Usable::NeedsReauth,
        Some((login_name, app_password, _)) => Usable::Ready(Credentials {
            login_name,
            app_password,
        }),
    })
}

/// Kept rather than deleted: one bad response must not turn an outage into "you
/// never connected this".
pub async fn mark_needs_reauth(pool: &MySqlPool, user_id: &str) -> Result<()> {
    sqlx::query("UPDATE nc_credentials SET status = 'needs_reauth' WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// No Nextcloud round-trip.
pub async fn status(pool: &MySqlPool, user_id: &str) -> Result<LinkStatus> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT status FROM nc_credentials WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let status = row.map(|(s,)| s);
    Ok(match status.as_deref() {
        Some("active") => LinkStatus::Active,
        Some(_) => LinkStatus::NeedsReauth,
        None => LinkStatus::NotLinked,
    })
}
