//! Validation helpers for Codex / OpenAI OAuth URLs and callback query parameters.
//! Shared by daemon, coder, and desktop; must not log secrets.

use std::collections::HashSet;

/// Hosts allowed for `authorize_url` (HTTPS only).
pub fn authorize_url_allowed_hosts() -> HashSet<&'static str> {
    HashSet::from(["auth.openai.com", "login.microsoftonline.com", "openai.com"])
}

/// Returns `true` if `url` is HTTPS and its host is in the allowlist.
pub fn validate_authorize_url(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    if u.scheme() != "https" {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    authorize_url_allowed_hosts().contains(host)
}

/// Build loopback callback URL for Variant A proxy (Codex listener).
pub fn codex_callback_url(port: u16, code: &str, state: &str) -> Result<String, String> {
    if code.is_empty() || state.is_empty() {
        return Err("code and state must be non-empty".into());
    }
    let mut u = url::Url::parse(&format!("http://127.0.0.1:{port}/auth/callback"))
        .map_err(|e| e.to_string())?;
    u.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", state);
    Ok(u.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_authorize_url_accepts_openai_auth() {
        // Given
        let url = "https://auth.openai.com/oauth/authorize?client_id=x";

        // When
        let result = validate_authorize_url(url);

        // Then
        assert!(result, "HTTPS openai auth URL must be accepted");
    }

    #[test]
    fn validate_authorize_url_rejects_http() {
        // Given
        let url = "http://auth.openai.com/oauth/authorize";

        // When
        let result = validate_authorize_url(url);

        // Then
        assert!(!result, "HTTP (non-HTTPS) authorize URLs must be rejected");
    }

    #[test]
    fn validate_authorize_url_rejects_unknown_host() {
        // Given
        let url = "https://evil.com/callback";

        // When
        let result = validate_authorize_url(url);

        // Then
        assert!(!result, "URLs with non-allowlisted hosts must be rejected");
    }

    #[test]
    fn codex_callback_url_builds_query() {
        // Given
        let port = 1455u16;

        // When
        let s = codex_callback_url(port, "abc", "xyz").unwrap();

        // Then
        assert!(s.starts_with("http://127.0.0.1:1455/auth/callback?"), "callback URL must use loopback address with /auth/callback path");
        assert!(s.contains("code=abc"), "callback URL must include code query param");
        assert!(s.contains("state=xyz"), "callback URL must include state query param");
    }

    #[test]
    fn codex_callback_url_rejects_empty_code() {
        // Given
        let empty_code = "";

        // When
        let result = codex_callback_url(1, empty_code, "x");

        // Then
        assert!(result.is_err(), "empty code must be rejected");
    }
}
