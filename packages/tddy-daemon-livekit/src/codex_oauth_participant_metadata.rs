//! Parse `codex_oauth` blob from LiveKit participant metadata (mirrors desktop `codex-oauth-metadata.ts`).

use serde::Deserialize;

/// Default loopback port when Codex prints `http://127.0.0.1:PORT/auth/callback`.
pub const DEFAULT_CODEX_OAUTH_CALLBACK_PORT: u16 = 1455;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexOAuthParticipantInfo {
    pub pending: bool,
    pub authorize_url: Option<String>,
    pub callback_port: Option<u16>,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexOAuthWire {
    pending: Option<bool>,
    authorize_url: Option<String>,
    callback_port: Option<serde_json::Value>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetadataRoot {
    codex_oauth: Option<CodexOAuthWire>,
}

fn callback_port_from_json(v: &serde_json::Value) -> Option<u16> {
    match v {
        serde_json::Value::Number(n) => {
            let u = n.as_u64()?;
            if u > 0 && u <= u64::from(u16::MAX) {
                Some(u as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Port for the OAuth callback listener; ignore missing / zero / out-of-range metadata.
pub fn resolved_codex_oauth_callback_port(info: &CodexOAuthParticipantInfo) -> u16 {
    match info.callback_port {
        Some(p) if p > 0 => p,
        _ => DEFAULT_CODEX_OAUTH_CALLBACK_PORT,
    }
}

/// Parse top-level participant metadata JSON for a `codex_oauth` object.
pub fn parse_codex_oauth_metadata(metadata: &str) -> Option<CodexOAuthParticipantInfo> {
    let t = metadata.trim();
    if t.is_empty() {
        return None;
    }
    let root: MetadataRoot = serde_json::from_str(t).ok()?;
    let c = root.codex_oauth?;
    let callback_port = c.callback_port.as_ref().and_then(callback_port_from_json);
    Some(CodexOAuthParticipantInfo {
        pending: c.pending.unwrap_or(false),
        authorize_url: c.authorize_url.filter(|s| !s.is_empty()),
        callback_port,
        state: c.state.filter(|s| !s.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_documented_shape() {
        let json = r#"{"codex_oauth":{"pending":true,"authorize_url":"https://auth.example.com/o","callback_port":1455,"state":"s"}}"#;
        let got = parse_codex_oauth_metadata(json).expect("parse");
        assert!(got.pending);
        assert_eq!(
            got.authorize_url.as_deref(),
            Some("https://auth.example.com/o")
        );
        assert_eq!(got.callback_port, Some(1455));
        assert_eq!(got.state.as_deref(), Some("s"));
        assert_eq!(resolved_codex_oauth_callback_port(&got), 1455);
    }

    #[test]
    fn resolved_port_defaults_when_missing() {
        let info = CodexOAuthParticipantInfo {
            pending: true,
            authorize_url: Some("https://a".into()),
            callback_port: None,
            state: None,
        };
        assert_eq!(
            resolved_codex_oauth_callback_port(&info),
            DEFAULT_CODEX_OAUTH_CALLBACK_PORT
        );
    }

    #[test]
    fn parse_returns_none_without_codex_key() {
        assert!(parse_codex_oauth_metadata(r#"{"other":1}"#).is_none());
    }
}
