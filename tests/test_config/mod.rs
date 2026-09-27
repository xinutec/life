//! The `Config` every test that builds an `AppState` starts from. A test states
//! only the fields it is about (`Config { static_dir, ..test_config::config() }`),
//! so a new field is added here once rather than in every file.

use life::config::Config;

/// Inert defaults: the pool is lazy, and nothing below is reached unless a test
/// overrides it to be.
pub(crate) fn config() -> Config {
    Config {
        database_url: "mysql://life:life@127.0.0.1:3307/life".into(),
        session_secret: "test-secret".into(),
        bind_addr: "127.0.0.1:0".into(),
        nc_base_url: "https://nc.example".into(),
        nc_client_id: "id".into(),
        nc_client_secret: "secret".into(),
        nc_redirect_uri: "https://life.example/auth/callback".into(),
        static_dir: None,
        dev_login_user: None,
        house_scene: "scenes/house.json".into(),
        // No council feed: the bins route answers with an empty list.
        bins_ical_url: None,
        emotion_worker_token: None,
    }
}
