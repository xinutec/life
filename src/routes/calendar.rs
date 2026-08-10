//! Calendar HTTP surface. Read-only so far: when the bins go out.

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::State;
use chrono::Utc;

use crate::calendar::bins::{self, BinDay};
use crate::error::AppError;
use crate::session::AuthUser;
use crate::state::AppState;

/// GET /api/bins → upcoming collections, soonest first.
///
/// An empty list when no feed is configured, rather than a 404: "your council
/// does not publish one" and "nothing is collected in the next three months"
/// are both simply nothing to show, and a client that had to tell them apart
/// would be doing it to render the same empty space.
///
/// A fetch that FAILS is an error, though. Answering an unreachable council
/// with an empty list would say the bins are not going out, which is the one
/// wrong thing this can say.
pub async fn bins(
    State(app): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<BinDay>>, AppError> {
    let Some(url) = app.cfg.bins_ical_url.as_deref() else {
        return Ok(Json(Vec::new()));
    };
    let ics = match app.cached_bins() {
        Some(ics) => ics,
        None => {
            let fetched = fetch(&app, url).await?;
            app.cache_bins(fetched.clone());
            fetched
        }
    };
    // Filtered against today on every read, not at fetch time: the cache
    // outlives midnight, and a collection is only "upcoming" relative to the
    // day you ask.
    Ok(Json(bins::upcoming(&ics, Utc::now().date_naive())?))
}

/// Fetch the feed. Every failure says it was the council's end, because that is
/// the difference between "look at the council's website" and "look at this
/// codebase" for whoever reads the log.
async fn fetch(app: &AppState, url: &str) -> Result<String> {
    let res = app
        .http
        .get(url)
        .send()
        .await
        .context("reaching the bin calendar")?;
    let status = res.status();
    if !status.is_success() {
        bail!("the bin calendar answered HTTP {status}");
    }
    res.text().await.context("reading the bin calendar")
}
