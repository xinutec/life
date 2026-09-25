//! Post-login redirect allowlist — must not become an open redirect — and the
//! calendar-link responses' wire shape.

use life::nextcloud::credentials::LinkStatus;
use life::routes::auth::{ConnectStarted, ConnectState, validate_return_to};

#[test]
fn allows_internal_paths() {
    assert_eq!(validate_return_to(Some("/recipes")), "/recipes");
    assert_eq!(validate_return_to(Some("/items?id=4")), "/items?id=4");
}

#[test]
fn rejects_open_redirects_and_falls_back_to_root() {
    assert_eq!(validate_return_to(Some("//evil.example")), "/");
    // Browsers fold `\` to `/` in URLs, so `/\evil` is `//evil` in disguise.
    assert_eq!(validate_return_to(Some("/\\evil.example")), "/");
    assert_eq!(validate_return_to(Some("https://evil.example")), "/");
    assert_eq!(validate_return_to(Some("evil")), "/");
    assert_eq!(validate_return_to(None), "/");
}

#[test]
fn calendar_link_responses_keep_their_wire_keys() {
    let started = ConnectStarted {
        login_url: "https://nc.example/login/v2/flow/x".into(),
    };
    assert_eq!(
        serde_json::to_value(&started).unwrap(),
        serde_json::json!({ "login_url": "https://nc.example/login/v2/flow/x" })
    );
    let state = ConnectState {
        status: LinkStatus::NeedsReauth,
    };
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        serde_json::json!({ "status": "needs_reauth" })
    );
}
