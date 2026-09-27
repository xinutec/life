//! NC OAuth2 authorize-URL construction — the browser-facing redirect.

mod test_config;

use life::nextcloud::identity::authorize_url;

#[test]
fn authorize_url_has_expected_params() {
    let url = url::Url::parse(&authorize_url(&test_config::config(), "st8")).unwrap();
    assert_eq!(url.path(), "/index.php/apps/oauth2/authorize");
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q.get("client_id").map(String::as_str), Some("id"));
    assert_eq!(q.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(q.get("state").map(String::as_str), Some("st8"));
    assert_eq!(
        q.get("redirect_uri").map(String::as_str),
        Some("https://life.example/auth/callback")
    );
    // The client secret must never appear in the browser-facing URL.
    assert!(!url.as_str().contains("secret"));
}
