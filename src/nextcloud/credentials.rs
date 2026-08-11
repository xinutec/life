//! Store for the NC app password (Login Flow v2 result), in life's own DB.
//! Single credential per user; no expiry, no refresh.

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

/// Upsert the app password granted via Login Flow v2.
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

/// The stored app password, ready to sign a CalDAV request.
pub struct Credentials {
    pub login_name: String,
    pub app_password: String,
}

/// What signing a CalDAV request with what we hold would actually get you.
///
/// Three outcomes rather than an `Option`, because the two empty-handed ones
/// need different things from the user: [`NotLinked`](Usable::NotLinked) wants
/// the grant run for the first time, and [`NeedsReauth`](Usable::NeedsReauth)
/// wants the existing one replaced. Telling someone "connect your calendar"
/// when it *is* connected and merely stale sends them looking for a button that
/// says something else.
pub enum Usable {
    /// No grant has ever completed — Login Flow v2 has not been run.
    NotLinked,
    /// There is a password, but Nextcloud has already rejected it once. Trying
    /// it again would fail the same way, so we don't.
    NeedsReauth,
    Ready(Credentials),
}

/// Read the credential for a DAV request, in one query.
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

/// Record that Nextcloud rejected the stored password.
///
/// The password is left in place rather than deleted: it is the evidence that a
/// grant once happened, `store` overwrites it on the next link anyway, and a row
/// that vanishes on one bad response would turn a Nextcloud outage into "you
/// never connected this".
pub async fn mark_needs_reauth(pool: &MySqlPool, user_id: &str) -> Result<()> {
    sqlx::query("UPDATE nc_credentials SET status = 'needs_reauth' WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cheap status read for /api/me — no NC round-trip.
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
