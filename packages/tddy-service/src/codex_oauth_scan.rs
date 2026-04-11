//! Heuristic scan of terminal output for Codex OAuth authorize URL and callback port.

use std::sync::{Arc, Mutex};

use crate::codex_oauth_validate::validate_authorize_url;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexOAuthDetected {
    pub authorize_url: String,
    pub callback_port: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub state: String,
}

/// Pending OAuth session on the agent host (memory only), updated by terminal scan for LiveKit metadata.
#[derive(Debug, Clone)]
pub struct CodexOAuthPending {
    pub detected: CodexOAuthDetected,
}

#[derive(Default)]
pub struct CodexOAuthSessionState {
    pub pending: Option<CodexOAuthPending>,
}

pub type CodexOAuthSession = Arc<Mutex<CodexOAuthSessionState>>;

/// Append UTF-8 chunk to buffer, trim to last `max_keep` bytes for rolling scan.
pub fn append_terminal_scan_buffer(buffer: &mut String, chunk: &[u8], max_keep: usize) {
    if let Ok(s) = std::str::from_utf8(chunk) {
        buffer.push_str(s);
    } else {
        buffer.push('\u{fffd}');
    }
    if buffer.len() > max_keep {
        let trim = buffer.len() - max_keep;
        buffer.drain(..trim);
    }
}

/// When only the HTTPS authorize URL is known (e.g. contents of `codex_oauth_authorize.url` from the
/// BROWSER hook), derive detection for LiveKit metadata. Callback port defaults to
/// 1455; `state` comes from the authorize URL query. Terminal output may later upgrade port/state.
pub fn codex_oauth_from_authorize_url_only(url: &str) -> Option<CodexOAuthDetected> {
    let url = url.trim();
    if url.is_empty() || !validate_authorize_url(url) {
        return None;
    }
    Some(CodexOAuthDetected {
        authorize_url: url.to_string(),
        callback_port: 1455,
        state: extract_state_from_authorize_url(url).unwrap_or_default(),
    })
}

/// Scan buffer for HTTPS authorize URL (allowlisted) and optional `127.0.0.1:PORT/auth/callback`.
pub fn scan_codex_oauth_from_buffer(buffer: &str) -> Option<CodexOAuthDetected> {
    let authorize_url = extract_authorize_url(buffer)?;
    if !validate_authorize_url(&authorize_url) {
        return None;
    }
    let callback_port = extract_callback_port(buffer).unwrap_or(1455);
    let state = extract_state_from_authorize_url(&authorize_url).unwrap_or_default();
    Some(CodexOAuthDetected {
        authorize_url,
        callback_port,
        state,
    })
}

fn extract_authorize_url(s: &str) -> Option<String> {
    // Look for https://auth.openai.com... (non-greedy until whitespace or quote)
    let needle = "https://";
    let mut start = 0;
    while let Some(i) = s[start..].find(needle) {
        let abs = start + i;
        let rest = &s[abs..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')')
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim_end_matches([',', '.']);
        if validate_authorize_url(candidate) {
            return Some(candidate.to_string());
        }
        start = abs + needle.len();
    }
    None
}

fn extract_callback_port(s: &str) -> Option<u16> {
    // http://127.0.0.1:PORT/auth/callback or http://localhost:PORT/auth/callback
    for pat in ["http://127.0.0.1:", "http://localhost:"] {
        if let Some(i) = s.rfind(pat) {
            let after = &s[i + pat.len()..];
            let port_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = port_str.parse::<u16>() {
                if after[port_str.len()..].starts_with("/auth/callback") {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn extract_state_from_authorize_url(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    u.query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_authorize_url_only_defaults_port_and_state() {
        let d = codex_oauth_from_authorize_url_only(
            "https://auth.openai.com/oauth/authorize?state=xyz&client_id=a",
        )
        .expect("detected");
        assert_eq!(d.callback_port, 1455);
        assert_eq!(d.state, "xyz");
    }

    #[test]
    fn scan_finds_authorize_and_port() {
        let mut b = String::new();
        let chunk = br#"Visit https://auth.openai.com/oauth/authorize?state=abc&client_id=x
Listening on http://127.0.0.1:8765/auth/callback
"#;
        append_terminal_scan_buffer(&mut b, chunk, 4096);
        let d = scan_codex_oauth_from_buffer(&b).expect("detected");
        assert!(d.authorize_url.contains("auth.openai.com"));
        assert_eq!(d.callback_port, 8765);
        assert_eq!(d.state, "abc");
    }
}
