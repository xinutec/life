//! Calendar HTTP surface: when the bins go out, and putting a shop trip in the
//! diary.

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

/// How long a shop takes, absent a stated answer.
const DEFAULT_MINUTES: i64 = 60;

/// The most items a client may send. A Buy list is a trolley; a request with
/// more than this in it is not one, and the description is bounded anyway
/// ([`trip`]) — this bounds the *request*.
const MAX_ITEMS: usize = 500;

/// Plan a shop trip: what, where, when, and what to bring home.
#[derive(Debug, Deserialize)]
pub struct NewShopTrip {
    pub shop: String,
    pub starts_at: DateTime<Utc>,
    pub minutes: Option<i64>,
    /// The Buy list as the person can see it right now. Sent by the client
    /// rather than read from the server's own rows on purpose: the list is
    /// local-first, so the phone's copy may be ahead of the sync, and an event
    /// that listed something other than the screen it was planned from would be
    /// wrong in the shop — which is the only place it gets read.
    #[serde(default)]
    pub items: Vec<String>,
}

/// Where the trip ended up.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PlannedTrip {
    /// The calendar it went into, by its own display name — life chooses, so it
    /// has to say which, or "added to your calendar" is unverifiable.
    pub calendar: String,
    /// The title as it will appear, so the confirmation quotes the event rather
    /// than describing it.
    pub summary: String,
}

/// POST /api/calendar/shop-trip → write the `VEVENT`, report where it went.
///
/// Nothing is recorded on this side. The event is the record, Nextcloud holds
/// it, and every calendar client the household already has shows it (§5).
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
    // The UID is ours and permanent — it is the identity of this event on every
    // device that syncs it, so it is minted once here rather than derived from
    // anything that could repeat (a shop and a time repeat every week).
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

/// Turn a CalDAV failure into a response, recording a rejected password on the
/// way past.
///
/// The recording matters as much as the status: until the row says
/// `needs_reauth`, /api/me keeps answering "active" and the app keeps offering
/// a calendar that cannot be written to. Failing to write the flag must not
/// mask the failure that revealed it, so it is logged and the original error is
/// still what comes back.
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
