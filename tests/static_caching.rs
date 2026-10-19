//! Only a file whose name carries its content hash may be cached for ever. An
//! icon, the manifest or the service worker's own files keep their name across
//! builds, so a year-long `immutable` would pin the old one on every device.

mod test_config;

use axum::body::Body;
use axum::http::{Request, header};
use life::config::Config;
use life::routes;
use life::state::AppState;
use tower::ServiceExt;

/// A static dir shaped like a real `ng build` output.
struct StaticDir(std::path::PathBuf);

impl StaticDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "life-caching-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(dir.join("media")).expect("create static dir");
        for (name, body) in [
            ("index.html", "<!doctype html><html></html>"),
            ("main-7GNM4EPX.js", "export {};"),
            ("media/roboto-latin-400-normal-ABCD1234.woff2", "font"),
            ("icon-192.png", "png"),
            ("manifest.webmanifest", "{}"),
            ("ngsw.json", "{}"),
            ("ngsw-worker.js", "//"),
        ] {
            std::fs::write(dir.join(name), body).expect("write");
        }
        Self(dir)
    }
}

impl Drop for StaticDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn cache_control(path: &str) -> String {
    let dir = StaticDir::new();
    let cfg = Config {
        static_dir: Some(dir.0.to_string_lossy().into_owned()),
        ..test_config::config()
    };
    let pool = sqlx::MySqlPool::connect_lazy(&cfg.database_url).expect("lazy pool");
    let res = routes::router(AppState::new(pool, cfg, reqwest::Client::new()))
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(res.status().is_success(), "{path}: {}", res.status());
    res.headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default()
}

#[tokio::test]
async fn content_hashed_files_are_immutable() {
    for path in [
        "/main-7GNM4EPX.js",
        "/media/roboto-latin-400-normal-ABCD1234.woff2",
    ] {
        let cc = cache_control(path).await;
        assert!(cc.contains("immutable"), "{path}: {cc}");
    }
}

#[tokio::test]
async fn files_that_keep_their_name_are_revalidated() {
    for path in [
        "/",
        "/today",
        "/index.html",
        "/icon-192.png",
        "/manifest.webmanifest",
        "/ngsw.json",
        "/ngsw-worker.js",
    ] {
        let cc = cache_control(path).await;
        assert_eq!(cc, "no-cache", "{path}");
    }
}
