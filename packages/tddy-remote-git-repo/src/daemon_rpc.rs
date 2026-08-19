//! The daemon's Connect-HTTP leg: the two unary calls this client makes before it touches LiveKit.
//!
//! `RefreshSession` turns a 7-day refresh token into an access token, and `MintLiveKitToken` turns
//! that access token into a room JWT. Both go over plain HTTP POSTs to `{base}/rpc/{service}/
//! {method}` with a protobuf body — the Connect protocol's unary shape, and the same path
//! `tddy-tools`' pty-relay uses.
//!
//! The mint is why this leg exists at all: the LiveKit API secret is also the HMAC key every
//! daemon signs session tokens with, so a client that minted its own room JWT would be holding a
//! credential that can impersonate any GitHub user on the fleet.

use std::error::Error;
use std::time::Duration;

use tddy_service::proto::auth::{
    MintLiveKitTokenRequest, MintLiveKitTokenResponse, RefreshSessionRequest,
    RefreshSessionResponse,
};

/// Where the daemon's LiveKit room is, and the JWT that admits this client to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomAdmission {
    pub token: String,
    pub url: String,
    pub room: String,
}

/// Why a call to the daemon's HTTP surface did not produce an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonRpcError {
    /// The daemon could not be reached, or the call did not complete.
    Unreachable { url: String, reason: String },
    /// The daemon answered, and refused. `code` is the Connect status code.
    Refused {
        method: &'static str,
        code: String,
        message: String,
    },
    /// The daemon answered with something that is not the response this method returns.
    Malformed {
        method: &'static str,
        reason: String,
    },
}

impl std::fmt::Display for DaemonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonRpcError::Unreachable { url, reason } => {
                write!(f, "could not reach the daemon at {url}: {reason}")
            }
            // The daemon's own message is the entire diagnostic a user gets for a rejected
            // credential, so it is surfaced verbatim rather than summarised.
            DaemonRpcError::Refused {
                method,
                code,
                message,
            } => write!(f, "the daemon refused {method} ({code}): {message}"),
            DaemonRpcError::Malformed { method, reason } => {
                write!(f, "could not read the daemon's {method} response: {reason}")
            }
        }
    }
}

/// A client for one daemon's Connect-HTTP surface.
pub struct DaemonRpc {
    http: reqwest::Client,
    base_url: String,
}

impl DaemonRpc {
    /// `base_url` is the daemon's HTTP root — `/rpc/<service>/<method>` is appended to it.
    ///
    /// `timeout` bounds each call. It is the same budget the LiveKit participant wait uses: both
    /// are "how long before this remote is declared unreachable", and git only ever sees the sum.
    pub fn new(base_url: &str, timeout: Duration) -> Result<Self, DaemonRpcError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| DaemonRpcError::Unreachable {
                url: base_url.to_string(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Exchange a refresh token for the short-lived access token every daemon RPC carries.
    pub async fn refresh_session(&self, refresh_token: &str) -> Result<String, DaemonRpcError> {
        let response: RefreshSessionResponse = self
            .unary(
                "auth.AuthService",
                "RefreshSession",
                RefreshSessionRequest {
                    refresh_token: refresh_token.to_string(),
                },
            )
            .await?;
        if response.session_token.is_empty() {
            return Err(DaemonRpcError::Malformed {
                method: "RefreshSession",
                reason: "it carried no access token".to_string(),
            });
        }
        Ok(response.session_token)
    }

    /// Ask the daemon for a JWT admitting this client to its room. The room and the participant
    /// identity are the daemon's choice; there is no request field for either.
    pub async fn mint_room_admission(
        &self,
        session_token: &str,
    ) -> Result<RoomAdmission, DaemonRpcError> {
        let response: MintLiveKitTokenResponse = self
            .unary(
                "auth.LiveKitTokenService",
                "MintLiveKitToken",
                MintLiveKitTokenRequest {
                    session_token: session_token.to_string(),
                },
            )
            .await?;
        // A blank field here would surface much later as an unexplained room-connect failure.
        for (name, value) in [
            ("token", &response.token),
            ("url", &response.url),
            ("room", &response.room),
        ] {
            if value.is_empty() {
                return Err(DaemonRpcError::Malformed {
                    method: "MintLiveKitToken",
                    reason: format!("it carried no {name}"),
                });
            }
        }
        Ok(RoomAdmission {
            token: response.token,
            url: response.url,
            room: response.room,
        })
    }

    async fn unary<Req: prost::Message, Res: prost::Message + Default>(
        &self,
        service: &str,
        method: &'static str,
        request: Req,
    ) -> Result<Res, DaemonRpcError> {
        let url = format!("{}/rpc/{service}/{method}", self.base_url);
        let response = self
            .http
            .post(&url)
            .header("content-type", "application/proto")
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| DaemonRpcError::Unreachable {
                url: url.clone(),
                reason: error_chain(&e),
            })?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| DaemonRpcError::Unreachable {
                url,
                reason: format!("read the response body: {e}"),
            })?;

        if !status.is_success() {
            return Err(connect_error(method, status, &body));
        }
        Res::decode(&body[..]).map_err(|e| DaemonRpcError::Malformed {
            method,
            reason: e.to_string(),
        })
    }
}

/// Read a Connect error body — `{"code": "unauthenticated", "message": "…"}` — falling back to the
/// HTTP status when the daemon answered with something else entirely (a proxy's error page, say).
fn connect_error(method: &'static str, status: reqwest::StatusCode, body: &[u8]) -> DaemonRpcError {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let field = |name: &str| -> Option<String> {
        parsed
            .as_ref()?
            .get(name)?
            .as_str()
            .map(std::string::ToString::to_string)
    };
    DaemonRpcError::Refused {
        method,
        code: field("code").unwrap_or_else(|| status.as_str().to_string()),
        message: field("message")
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string()),
    }
}

/// Walk an error's `source` chain so a transport failure names its root cause (timeout, connect
/// refused, TLS, …) instead of stopping at reqwest's generic "error sending request for url".
fn error_chain(err: &reqwest::Error) -> String {
    let mut parts: Vec<String> = vec![err.to_string()];
    let mut current: Option<&dyn std::error::Error> = err.source();
    while let Some(source) = current {
        let msg = source.to_string();
        if !parts.iter().any(|p| p == &msg) {
            parts.push(msg);
        }
        current = source.source();
    }
    parts.join(": caused by ")
}
