//! When the bins go out, and putting a shop trip in the diary.

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use ulid::Ulid;

use crate::calendar::bins::{self, BinDay};
use crate::calendar::caldav::{Dav, DavError};
use crate::calendar::trip::{self, ShopTrip};
use crate::error::AppError;
use crate::nextcloud::credentials::{self, Usable};
use crate::session::AuthUser;
use crate::state::AppState;

/// GET /api/bins → upcoming collections, soonest first; empty without a feed. A
/// failed fetch is an error: an empty list would say the bins are not going out.
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
    // Filtered on every read: the cache outlives midnight.
    Ok(Json(bins::upcoming(&ics, Utc::now().date_naive())?))
}

const DEFAULT_MINUTES: i64 = 60;

/// Bounds the request; the description is bounded separately ([`trip`]).
const MAX_ITEMS: usize = 500;

#[derive(Debug, Deserialize)]
pub struct NewShopTrip {
    pub shop: String,
    pub starts_at: DateTime<Utc>,
    pub minutes: Option<i64>,
    /// The Buy list as the phone shows it, which may be ahead of the sync.
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PlannedTrip {
    /// By display name, so "added to your calendar" can be checked.
    pub calendar: String,
    pub summary: String,
}

/// POST /api/calendar/shop-trip → write the `VEVENT`. Nothing is kept here: the
/// event is the record.
pub async fn plan_shop_trip(
    State(app): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewShopTrip>,
) -> Result<Json<PlannedTrip>, AppError> {
    if body.items.len() > MAX_ITEMS {
        return Err(AppError::BadRequest(format!(
            "a shop trip carries at most {MAX_ITEMS} items"
        )));
    }
    let creds = match credentials::for_dav(&app.pool, &user.user_id).await? {
        Usable::NotLinked => return Err(AppError::NcNotLinked),
        Usable::NeedsReauth => return Err(AppError::NcReauthRequired),
        Usable::Ready(creds) => creds,
    };

    let planned = ShopTrip {
        shop: body.shop.trim().to_string(),
        starts_at: body.starts_at,
        minutes: body.minutes.unwrap_or(DEFAULT_MINUTES),
        items: body.items,
    };
    // Minted once: a shop and a time repeat every week.
    let uid = format!("shop-trip-{}@life", Ulid::new());
    let ics =
        trip::ics(&planned, &uid, Utc::now()).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let dav = Dav::new(&app.http, &app.cfg.nc_base_url, &creds);
    let calendar = match dav.writable_calendar().await {
        Ok(calendar) => calendar,
        Err(e) => return Err(dav_failed(&app, &user.user_id, e).await),
    };
    if let Err(e) = dav.put_event(&calendar, &uid, &ics).await {
        return Err(dav_failed(&app, &user.user_id, e).await);
    }
    tracing::info!(
        "shop trip written to {} for {}",
        calendar.href,
        user.user_id
    );

    Ok(Json(PlannedTrip {
        summary: trip::summary(&planned.shop),
        calendar: calendar.name,
    }))
}

/// Records a rejected password, or /api/me keeps offering a calendar that cannot
/// be written to.
async fn dav_failed(app: &AppState, user_id: &str, err: DavError) -> AppError {
    match err {
        DavError::Unauthorized => {
            if let Err(e) = credentials::mark_needs_reauth(&app.pool, user_id).await {
                tracing::error!("recording the rejected NC app password: {e:#}");
            }
            AppError::NcReauthRequired
        }
        DavError::Other(e) => {
            tracing::error!("caldav: {e:#}");
            AppError::Upstream(format!("{e:#}"))
        }
    }
}

/// Every failure names the council's end, so the log says where to look.
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
