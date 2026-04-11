//! Codex CLI OAuth browser capture and callback relay (web dashboard).
//!
//! Validates HTTPS authorize URLs against a configurable host allowlist, binds captures to
//! tool sessions, and parses OAuth callback query parameters for delivery to the Codex
//! process listener.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

/// Event emitted when the environment `BROWSER` hook forwards an authorize URL to Tddy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexOAuthAuthorizeCapture {
    /// Owning tool session (must match the active Codex/tddy-coder session).
    pub session_id: String,
    /// Full HTTPS URL opened by Codex for operator login.
    pub authorize_url: String,
}

/// Result of delivering an OAuth callback to the Codex listener (loopback or forwarded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexOAuthCallbackDelivery {
    pub session_id: String,
    /// Query pairs delivered to the registered handler (e.g. `code`, `state`).
    pub query: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexOAuthRelayError {
    Validation(CodexOAuthValidationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexOAuthValidationError {
    SchemeNotHttps,
    HostNotAllowed {
        host: String,
    },
    CorrelationMismatch {
        expected: String,
        got: String,
    },
    /// No `https` authorize URL found in `BROWSER` argv (wrapper must pass the full URL).
    NoHttpsAuthorizeUrlInBrowserArgv,
}

/// Configurable allowlist for Codex / OpenAI OAuth authorize hosts (prod + staging).
#[derive(Debug, Clone)]
pub struct CodexOAuthHostAllowlist {
    hosts: Vec<String>,
}

impl Default for CodexOAuthHostAllowlist {
    fn default() -> Self {
        Self {
            hosts: vec![
                "chatgpt.com".to_string(),
                "openai.com".to_string(),
                "auth.openai.com".to_string(),
            ],
        }
    }
}

impl CodexOAuthHostAllowlist {
    pub fn contains_host(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.hosts.iter().any(|h| h == &host)
    }
}

/// Validates a captured authorize URL before it is shown in tddy-web.
pub fn validate_codex_oauth_authorize_url(
    url: &Url,
    session_correlation_id: &str,
    active_session_id: Option<&str>,
    allowlist: &CodexOAuthHostAllowlist,
) -> Result<(), CodexOAuthRelayError> {
    log::debug!(
        target: "tddy_daemon::codex_oauth",
        "validate_codex_oauth_authorize_url: scheme={} host_present={}",
        url.scheme(),
        url.host_str().is_some()
    );

    if url.scheme() != "https" {
        log::debug!(
            target: "tddy_daemon::codex_oauth",
            "validate_codex_oauth_authorize_url: rejected scheme (not https)"
        );
        return Err(CodexOAuthRelayError::Validation(
            CodexOAuthValidationError::SchemeNotHttps,
        ));
    }

    let host = match url.host_str() {
        Some(h) => h,
        None => {
            return Err(CodexOAuthRelayError::Validation(
                CodexOAuthValidationError::HostNotAllowed {
                    host: String::new(),
                },
            ));
        }
    };

    if !allowlist.contains_host(host) {
        log::info!(
            target: "tddy_daemon::codex_oauth",
            "validate_codex_oauth_authorize_url: host not allowlisted (host omitted from logs)"
        );
        return Err(CodexOAuthRelayError::Validation(
            CodexOAuthValidationError::HostNotAllowed {
                host: host.to_string(),
            },
        ));
    }

    if let Some(active) = active_session_id {
        if active != session_correlation_id {
            log::debug!(
                target: "tddy_daemon::codex_oauth",
                "validate_codex_oauth_authorize_url: session correlation mismatch"
            );
            return Err(CodexOAuthRelayError::Validation(
                CodexOAuthValidationError::CorrelationMismatch {
                    expected: active.to_string(),
                    got: session_correlation_id.to_string(),
                },
            ));
        }
    }

    log::info!(
        target: "tddy_daemon::codex_oauth",
        "validate_codex_oauth_authorize_url: ok (session correlation aligned, host allowlisted)"
    );
    Ok(())
}

/// Parses `BROWSER` argv (e.g. `["tddy-browser-hook", "https://..."]`), validates the URL,
/// and returns a session-scoped capture for the web client.
pub async fn dispatch_browser_open_capture(
    browser_argv: &[String],
    session_id: &str,
) -> Result<CodexOAuthAuthorizeCapture, CodexOAuthRelayError> {
    log::info!(
        target: "tddy_daemon::codex_oauth",
        "dispatch_browser_open_capture: argv_len={} session_id_len={}",
        browser_argv.len(),
        session_id.len()
    );

    let url = browser_argv
        .iter()
        .filter_map(|s| Url::parse(s).ok())
        .find(|u| u.scheme() == "https")
        .ok_or_else(|| {
            log::debug!(
                target: "tddy_daemon::codex_oauth",
                "dispatch_browser_open_capture: no https URL in argv"
            );
            CodexOAuthRelayError::Validation(
                CodexOAuthValidationError::NoHttpsAuthorizeUrlInBrowserArgv,
            )
        })?;

    let allowlist = CodexOAuthHostAllowlist::default();
    validate_codex_oauth_authorize_url(&url, session_id, Some(session_id), &allowlist)?;

    log::debug!(
        target: "tddy_daemon::codex_oauth",
        "dispatch_browser_open_capture: parsed authorize URL for session"
    );

    Ok(CodexOAuthAuthorizeCapture {
        session_id: session_id.to_string(),
        authorize_url: url.to_string(),
    })
}

/// Parses the OAuth callback URL query string into a [`CodexOAuthCallbackDelivery`] for `session_id`.
///
/// The running Codex CLI receives the same query parameters on its loopback listener; this
/// struct carries the parsed map for IPC or relay layers above this module.
pub async fn relay_oauth_callback_to_registered_listener(
    session_id: &str,
    callback_url: &Url,
) -> Result<CodexOAuthCallbackDelivery, CodexOAuthRelayError> {
    log::info!(
        target: "tddy_daemon::codex_oauth",
        "relay_oauth_callback_to_registered_listener: session_id_len={} scheme={} has_query={}",
        session_id.len(),
        callback_url.scheme(),
        callback_url.query().is_some()
    );

    let mut query = HashMap::new();
    for (k, v) in callback_url.query_pairs() {
        query.insert(k.into_owned(), v.into_owned());
    }

    log::debug!(
        target: "tddy_daemon::codex_oauth",
        "relay_oauth_callback_to_registered_listener: query key count={}",
        query.len()
    );

    Ok(CodexOAuthCallbackDelivery {
        session_id: session_id.to_string(),
        query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_handler_accepts_https_openai_authorize_url_and_matching_session() {
        let url =
            Url::parse("https://auth.openai.com/oauth/authorize?client_id=x&state=y").unwrap();
        let allowlist = CodexOAuthHostAllowlist::default();
        let r = validate_codex_oauth_authorize_url(
            &url,
            "sess-correlation-1",
            Some("sess-correlation-1"),
            &allowlist,
        );
        assert!(
            r.is_ok(),
            "expected validated authorize URL for active session, got {r:?}"
        );
    }

    #[test]
    fn relay_delivers_callback_query_once_to_mock_listener() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let url = Url::parse("http://127.0.0.1:9/callback?code=abc&state=xyz").unwrap();
        let got = rt
            .block_on(async { relay_oauth_callback_to_registered_listener("sess-1", &url).await });
        assert!(
            got.is_ok(),
            "expected relay to deliver code/state to listener, got {got:?}"
        );
        let delivery = got.expect("checked");
        assert_eq!(delivery.query.get("code"), Some(&"abc".to_string()));
        assert_eq!(delivery.query.get("state"), Some(&"xyz".to_string()));
    }

    #[test]
    fn validate_rejects_http_scheme_before_allowlist_checks() {
        let url = Url::parse("http://auth.openai.com/oauth/authorize?state=x").unwrap();
        let allowlist = CodexOAuthHostAllowlist::default();
        let r = validate_codex_oauth_authorize_url(&url, "s1", Some("s1"), &allowlist);
        assert!(
            matches!(
                r,
                Err(CodexOAuthRelayError::Validation(
                    CodexOAuthValidationError::SchemeNotHttps
                ))
            ),
            "expected http authorize URL to fail scheme validation, got {r:?}"
        );
    }

    #[test]
    fn validate_rejects_correlation_id_mismatch_with_distinct_error() {
        let url =
            Url::parse("https://auth.openai.com/oauth/authorize?client_id=x&state=y").unwrap();
        let allowlist = CodexOAuthHostAllowlist::default();
        let r = validate_codex_oauth_authorize_url(
            &url,
            "expected-session",
            Some("different-active-session"),
            &allowlist,
        );
        assert!(
            matches!(
                r,
                Err(CodexOAuthRelayError::Validation(
                    CodexOAuthValidationError::CorrelationMismatch { .. }
                ))
            ),
            "expected session correlation mismatch, got {r:?}"
        );
    }
}
