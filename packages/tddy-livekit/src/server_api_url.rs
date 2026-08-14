//! Deriving LiveKit's **server-API** base URL from the signalling URL in daemon config.
//!
//! `livekit.url` is a WebSocket address (`ws://host:7880`), but `RoomClient::with_api_key` wants an
//! HTTP one. The only converter in the tree before this was test-only, and it required an explicit
//! port — so a `wss://livekit.example.com` with no port would not have survived it. This one handles
//! the portless case by leaving the authority alone: an absent port means the scheme's default, and
//! `wss` → `https` maps 443 to 443.
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`

use std::fmt;

/// Why a configured LiveKit URL cannot be turned into a server-API base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerApiUrlError {
    /// The URL carried a scheme other than `ws` or `wss`.
    NotWebSocketScheme(String),
    /// The URL had no scheme separator, or nothing after it.
    Malformed(String),
}

impl fmt::Display for ServerApiUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWebSocketScheme(url) => {
                write!(f, "livekit url is not a ws:// or wss:// address: {url}")
            }
            Self::Malformed(url) => write!(f, "livekit url is malformed: {url}"),
        }
    }
}

impl std::error::Error for ServerApiUrlError {}

/// Turn a LiveKit signalling URL into the HTTP base its server API is served on.
///
/// `ws://` → `http://`, `wss://` → `https://`. Host, port and path are preserved untouched, so a
/// portless authority stays portless and a reverse-proxy path prefix survives.
pub fn http_base_from_ws_url(ws_url: &str) -> Result<String, ServerApiUrlError> {
    let malformed = || ServerApiUrlError::Malformed(ws_url.to_string());
    let (scheme, rest) = ws_url.split_once("://").ok_or_else(malformed)?;
    let http_scheme = match scheme {
        "ws" => "http",
        "wss" => "https",
        _ => return Err(ServerApiUrlError::NotWebSocketScheme(ws_url.to_string())),
    };
    // The authority runs up to the first path separator; an empty one means there is no host to
    // address, which no amount of scheme rewriting fixes.
    if rest.is_empty() || rest.starts_with('/') {
        return Err(malformed());
    }
    Ok(format!("{http_scheme}://{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_an_insecure_socket_url_to_http() {
        // Given the default dev configuration
        let url = "ws://127.0.0.1:7880";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Ok("http://127.0.0.1:7880".to_string()));
    }

    #[test]
    fn maps_a_secure_socket_url_to_https() {
        // Given a TLS deployment naming its port
        let url = "wss://livekit.example.com:443";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Ok("https://livekit.example.com:443".to_string()));
    }

    #[test]
    fn keeps_an_authority_that_names_no_port() {
        // Given a TLS deployment on the scheme's default port — the case the test-only converter
        // could not handle
        let url = "wss://livekit.example.com";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Ok("https://livekit.example.com".to_string()));
    }

    #[test]
    fn preserves_a_reverse_proxy_path_prefix() {
        // Given LiveKit served under a path
        let url = "wss://edge.example.com/livekit";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Ok("https://edge.example.com/livekit".to_string()));
    }

    #[test]
    fn rejects_a_url_that_is_already_http() {
        // Given an operator who configured the HTTP address by mistake
        let url = "http://127.0.0.1:7880";

        // When
        let base = http_base_from_ws_url(url);

        // Then the error names the offending value rather than silently passing it through
        assert_eq!(
            base,
            Err(ServerApiUrlError::NotWebSocketScheme(url.to_string()))
        );
    }

    #[test]
    fn rejects_a_url_with_no_scheme() {
        // Given a bare host
        let url = "127.0.0.1:7880";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Err(ServerApiUrlError::Malformed(url.to_string())));
    }

    #[test]
    fn rejects_a_scheme_with_no_authority() {
        // Given a truncated configuration value
        let url = "wss://";

        // When
        let base = http_base_from_ws_url(url);

        // Then
        assert_eq!(base, Err(ServerApiUrlError::Malformed(url.to_string())));
    }
}
