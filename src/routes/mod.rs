//! HTTP routing table.

pub mod api;
pub mod auth;
pub mod calendar;
pub mod conflicts;
pub mod emotion_worker;
pub mod inventory;
pub mod products;
pub mod recipes;
pub mod shopping;
pub mod sync;
pub mod telemetry;
pub mod todo;
pub mod trash;
pub mod wellbeing;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Response, header};
use axum::routing::{delete, get, patch, post};

use crate::error::AppError;
use crate::products::off;
use tower::ServiceBuilder;
use tower_http::services::fs::ServeFileSystemResponseBody;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::AppState;

/// How long a static response may be reused without asking again.
///
/// ⚠ **`index.html` MUST REVALIDATE, and shipping it without saying so cost a
/// deploy nobody could see.** With no `Cache-Control` at all a client falls back
/// to *heuristic* caching from `Last-Modified`, and is free to keep the document
/// for as long as it likes without ever asking again. MEASURED on `messages`
/// 2026-08-14: an Android WebView fetched the whole API — `/api/me`,
/// `/api/conversations`, a whole thread — and never once requested `main-*.js`.
/// The phone ran a build several deploys old for hours while the server had been
/// serving the new one all along.
///
/// ⚠ The symptom is "the change did not deploy", which sends you to CI, the
/// image tag, the rollout and the manifests — all of which are correct. What
/// identified it was a rendering detail that could only come from old code.
///
/// `no-cache` rather than `no-store`: it means "ask first", not "never keep", so
/// the ETag still turns the usual case into a 304 with no body.
///
/// Everything else Angular emits carries a content hash in its NAME, so a new
/// build is a new URL and the old one can never be wrong. Those are the one kind
/// of response `immutable` is honestly available for.
fn cache_control_for(res: &Response<ServeFileSystemResponseBody>) -> Option<HeaderValue> {
    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"));
    Some(if is_html {
        HeaderValue::from_static("no-cache")
    } else {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    })
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/me", get(api::me))
        .route("/house", get(api::house))
        // An ambient household fact like the house geometry, not an entity of
        // ours: read from the council, owned by nobody here.
        .route("/bins", get(calendar::bins))
        // Out, not in: the diary is Nextcloud's, and this writes one event to it.
        .route("/calendar/shop-trip", post(calendar::plan_shop_trip))
        .route("/nextcloud/connect/init", post(auth::connect_init))
        .route("/nextcloud/connect/status", get(auth::connect_status))
        .route(
            "/locations",
            get(inventory::list_locations).post(inventory::create_location),
        )
        .route("/locations/{id}", delete(inventory::delete_location))
        .route(
            "/items",
            get(inventory::list_items).post(inventory::create_item),
        )
        .route(
            "/items/{id}",
            patch(inventory::update_item).delete(inventory::delete_item),
        )
        .route("/items/{id}/history", get(inventory::item_history))
        .route("/items/{id}/move", post(inventory::move_item))
        .route("/items/{id}/use", post(inventory::use_item))
        .route("/recipes", get(recipes::list).post(recipes::create))
        .route(
            "/recipes/{id}",
            get(recipes::get_one)
                .put(recipes::update)
                .delete(recipes::delete),
        )
        .route("/recipes/{id}/shopping-list", get(recipes::shopping_list))
        .route("/recipes/{id}/cook", post(recipes::cook))
        .route("/cookable", get(recipes::cookable))
        .route("/shopping", get(shopping::list).post(shopping::create))
        .route(
            "/shopping/{id}",
            patch(shopping::update).delete(shopping::delete),
        )
        .route("/shopping/{id}/buy", post(shopping::buy))
        .route("/shopping/coverage", post(shopping::coverage))
        .route(
            "/sync/shopping",
            get(sync::pull_shopping).post(sync::push_shopping),
        )
        .route("/todo", get(todo::list).post(todo::create))
        .route("/todo/{id}", patch(todo::update).delete(todo::delete))
        .route("/sync/todo", get(sync::pull_todo).post(sync::push_todo))
        .route("/todo-links", get(todo::list_links).post(todo::create_link))
        .route("/todo-links/{id}", delete(todo::delete_link))
        .route(
            "/sync/todo-link",
            get(sync::pull_todo_link).post(sync::push_todo_link),
        )
        .route(
            "/sync/wellbeing",
            get(sync::pull_wellbeing).post(sync::push_wellbeing),
        )
        .route(
            "/wellbeing/suggest-emotions",
            post(wellbeing::suggest_emotions),
        )
        .route("/wellbeing/warm-emotions", post(wellbeing::warm_emotions))
        // The Mac's suggestion worker dials IN here — it holds the model, and the
        // fleet may not open connections toward it. Bearer token, not a session:
        // it is a daemon acting for nobody.
        .route("/emotion-worker/next", get(emotion_worker::next))
        .route("/emotion-worker/{id}/result", post(emotion_worker::result))
        .route("/telemetry", post(telemetry::record))
        .route("/conflicts", get(conflicts::list).post(conflicts::create))
        .route("/conflicts/{id}/resolve", post(conflicts::resolve))
        .route("/trash", get(trash::list))
        .route("/trash/{kind}/{ref}/restore", post(trash::restore))
        .route("/products", get(products::search))
        .route("/products/shop/asda", get(products::search_asda))
        .route(
            "/products/shop/{source}/listings",
            post(products::remember_seen),
        )
        .route("/products/import", post(products::import))
        .route("/products/id/{id}", get(products::product_detail))
        .route("/products/id/{id}/listings", post(products::sync_listing))
        .route("/products/id/{id}/facts", post(products::submit_facts))
        .route("/products/id/{id}/reconcile", post(products::reconcile))
        .route(
            "/products/id/{id}/find/{source}",
            get(products::find_at_shop),
        )
        .route("/products/id/{id}/image", get(products::image_by_id))
        .route("/products/{barcode}", get(products::lookup))
        .route(
            "/products/{barcode}/image",
            // Image uploads can be a few MiB; raise the default 2 MiB body limit
            // for THIS route only (the handler re-checks the real 5 MiB cap). The
            // GET side has no request body, so the raised limit is harmless there.
            get(products::image)
                .put(products::set_image)
                .layer(DefaultBodyLimit::max(off::MAX_UPLOAD_BYTES + 64 * 1024)),
        )
        // An unmatched /api path is a 404 in the API's own language. Without this
        // it would fall through to the SPA fallback below and answer 200 with
        // index.html — and a 2xx non-JSON body is precisely how the client's
        // `classifyFetchResponse` recognises a lapsed session, so a typo'd or
        // retired route would read to the sync layer as "you're logged out".
        .fallback(|| async { AppError::NotFound })
        // One INFO line per API request (method, path, status, latency). Scoped to
        // /api so static-asset serving and the k8s /healthz probe don't spam it.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/login", get(auth::login))
        .route("/auth/callback", get(auth::callback))
        .route("/logout", post(auth::logout))
        .nest("/api", api);

    // DEV ONLY: mount /dev-login only when DEV_LOGIN_USER is set.
    if state.cfg.dev_login_user.is_some() {
        app = app.route("/dev-login", get(auth::dev_login));
    }

    // Serve the built Angular bundle (single origin), falling back to
    // index.html so client-side routes resolve. API-only when STATIC_DIR unset.
    if let Some(dir) = state.cfg.static_dir.clone() {
        let serve = ServeDir::new(&dir).fallback(ServeFile::new(format!("{dir}/index.html")));
        // ⚠ The layer wraps only the STATIC service: an API response is neither
        // a document to revalidate nor an immutable asset, and giving JSON a
        // year-long `immutable` would be the same bug pointing the other way.
        let serve = ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                cache_control_for,
            ))
            .service(serve);
        app = app.fallback_service(serve);
    }

    app.with_state(state)
}
