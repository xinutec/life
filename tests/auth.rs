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

/// Where a browser goes is decided by its URL parser, which drops tabs and
/// newlines and folds `\` to `/`: `/\t/evil` is `//evil`. So the check is the
/// destination as that parser resolves it, not the string's first characters.
#[test]
fn every_accepted_path_resolves_to_this_site() {
    let site = url::Url::parse("https://life.example/").unwrap();
    for attempt in [
        "/\t/evil.example",
        "/\n/evil.example",
        "/\r/evil.example",
        "/\t\\evil.example",
        "/ /evil.example",
        "/recipes",
    ] {
        let dest = validate_return_to(Some(attempt));
        let landed = site.join(&dest).unwrap();
        assert_eq!(
            landed.host_str(),
            Some("life.example"),
            "{attempt:?} was let through as {dest:?}"
        );
    }
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
