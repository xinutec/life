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
use crate::files::types as files_types;
use crate::products::off;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::services::fs::ServeFileSystemResponseBody;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::AppState;

/// `index.html` revalidates, or clients run an old build for hours; everything
/// else is content-hashed, so `immutable`.
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

/// The app's page for a client-side route, but a 404 for anything that names a
/// file (a dot in the last segment): a woff2 answered with HTML fails silently.
fn spa(index: &str, path: &str) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    if path
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    match std::fs::read_to_string(index) {
        Ok(page) => axum::response::Html(page).into_response(),
        Err(error) => {
            // A STATIC_DIR without an index is a misconfiguration; say so.
            tracing::error!("the app's index could not be read: {error}");
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "no index").into_response()
        }
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/me", get(api::me))
        .route("/house", get(api::house))
        .route("/bins", get(calendar::bins))
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
        .route(
            "/items/{id}/files",
            get(inventory::list_files).post(inventory::add_file).layer(
                // Re-stated in the handler, in case the route is re-wired.
                DefaultBodyLimit::max(files_types::MAX_FILE_BYTES + 64 * 1024),
            ),
        )
        .route(
            "/items/{id}/files/{file_id}",
            get(inventory::get_file).delete(inventory::delete_file),
        )
        .route("/items/{id}/purchases", post(inventory::record_purchase))
        .route(
            "/items/{id}/purchases/{purchase_id}",
            delete(inventory::delete_purchase),
        )
        .route("/items/{id}/move", post(inventory::move_item))
        .route("/items/{id}/use", post(inventory::use_item))
        .route("/items/{id}/low", post(inventory::mark_low))
        .route("/items/low", post(inventory::mark_low_by_identity))
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
        .route("/shopping", get(shopping::list))
        .route("/shopping/{id}/buy", post(shopping::buy))
        .route("/shopping/coverage", post(shopping::coverage))
        .route(
            "/sync/shopping",
            get(sync::pull_shopping).post(sync::push_shopping),
        )
        .route("/todo", get(todo::list).post(todo::create))
        .route("/todo/{id}", patch(todo::update))
        .route("/sync/todo", get(sync::pull_todo).post(sync::push_todo))
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
        // The Mac's worker dials in here; the fleet may not dial the Mac.
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
            // Raised for this route only; the handler checks the real 5 MiB cap.
            get(products::image)
                .put(products::set_image)
                .layer(DefaultBodyLimit::max(off::MAX_UPLOAD_BYTES + 64 * 1024)),
        )
        // Without this an unknown /api path would get index.html, a 2xx non-JSON
        // body, which the client reads as a lapsed session.
        .fallback(|| async { AppError::NotFound })
        // One INFO line per API request; not for assets or /healthz.
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

    if state.cfg.dev_login_user.is_some() {
        app = app.route("/dev-login", get(auth::dev_login));
    }

    if let Some(dir) = state.cfg.static_dir.clone() {
        let index = format!("{dir}/index.html");
        let serve = ServeDir::new(&dir).fallback(get(move |uri: axum::http::Uri| {
            let index = index.clone();
            async move { spa(&index, uri.path()) }
        }));
        // Static files only: JSON is neither revalidated nor immutable.
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
