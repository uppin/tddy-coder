//! Acceptance tests for Codex OAuth browser capture → tddy-web → callback relay.
//! Exercises `tddy_daemon::codex_oauth_relay` validation, `BROWSER` argv dispatch, and callback parsing.

use tddy_daemon::codex_oauth_relay::{
    dispatch_browser_open_capture, relay_oauth_callback_to_registered_listener,
    validate_codex_oauth_authorize_url, CodexOAuthHostAllowlist,
};
use url::Url;

#[tokio::test]
async fn browser_capture_emits_session_scoped_authorize_url() {
    let session = "7b3e2f10-0000-7000-8000-000000000001";
    let argv = vec![
        "tddy-browser-hook".into(),
        "https://auth.openai.com/oauth/authorize?client_id=c&state=s".into(),
    ];
    let cap = dispatch_browser_open_capture(&argv, session)
        .await
        .expect("BROWSER hook must emit structured capture with HTTPS authorize URL");

    assert_eq!(cap.session_id, session);
    let parsed = Url::parse(&cap.authorize_url).expect("authorize URL must parse");
    assert_eq!(parsed.scheme(), "https");
    assert!(
        parsed
            .host_str()
            .is_some_and(|h| { CodexOAuthHostAllowlist::default().contains_host(h) }),
        "host must match configurable Codex OAuth allowlist, got {:?}",
        parsed.host_str()
    );
}

#[tokio::test]
async fn callback_relay_completes_handshake_for_mock_codex_listener() {
    let callback =
        Url::parse("http://127.0.0.1:54321/oauth/callback?code=mock-code&state=mock-state")
            .expect("callback URL");
    let delivery = relay_oauth_callback_to_registered_listener("sess-mock-1", &callback)
        .await
        .expect("callback must be delivered once to the session-registered Codex listener");

    assert_eq!(delivery.session_id, "sess-mock-1");
    assert_eq!(
        delivery.query.get("code").map(String::as_str),
        Some("mock-code")
    );
    assert_eq!(
        delivery.query.get("state").map(String::as_str),
        Some("mock-state")
    );
}

#[test]
fn capture_validation_enforces_https_scheme_allowlisted_host_and_session_correlation() {
    let authorize = Url::parse("https://auth.openai.com/oauth/authorize?state=st").unwrap();
    let allowlist = CodexOAuthHostAllowlist::default();
    validate_codex_oauth_authorize_url(
        &authorize,
        "active-session-z",
        Some("active-session-z"),
        &allowlist,
    )
    .expect("valid https authorize URL for same session must pass validation");
}
