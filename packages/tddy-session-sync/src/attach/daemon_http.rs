use std::time::Duration;
use tddy_service::proto::auth::{RefreshSessionRequest, RefreshSessionResponse};
use tddy_service::proto::session::{ListSessionsRequest, ListSessionsResponse, SessionEntry};
use tddy_service::proto::session_room::{OpenSessionRoomRequest, OpenSessionRoomResponse};

/// Why a call to the daemon's Connect-HTTP surface did not produce an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonHttpError {
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

impl std::fmt::Display for DaemonHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonHttpError::Unreachable { url, reason } => {
                write!(f, "could not reach the daemon at {url}: {reason}")
            }
            // The daemon's own message is the entire diagnostic a user gets for a rejected
            // credential, so it is surfaced verbatim rather than summarised.
            DaemonHttpError::Refused {
                method,
                code,
                message,
            } => write!(f, "the daemon refused {method} ({code}): {message}"),
            DaemonHttpError::Malformed { method, reason } => {
                write!(f, "could not read the daemon's {method} response: {reason}")
            }
        }
    }
}

impl std::error::Error for DaemonHttpError {}

/// The daemon's Connect-HTTP leg: the two unary calls made before the room is joined.
///
/// A protobuf POST to `{base}/rpc/{service}/{method}`, which is the Connect protocol's unary shape
/// — the same one `tddy-remote-git-repo` exchanges its token on and the same one the web dashboard
/// lists sessions on.
pub(super) struct DaemonHttp {
    http: reqwest::Client,
    base_url: String,
}

impl DaemonHttp {
    pub(super) fn new(base_url: &str, timeout: Duration) -> Result<Self, DaemonHttpError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| DaemonHttpError::Unreachable {
                url: base_url.to_string(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Exchange a 7-day refresh token for the 5-minute access token every RPC carries.
    pub(super) async fn refresh_session(
        &self,
        refresh_token: &str,
    ) -> Result<String, DaemonHttpError> {
        let response: RefreshSessionResponse = self
            .unary(
                "auth.AuthService",
                "RefreshSession",
                RefreshSessionRequest {
                    refresh_token: refresh_token.to_string(),
                    // A tool is not a browser session lineage: it was handed no vault unlock key,
                    // so it has none to present, and its refresh reopens no credential vault.
                    vault_unlock_key: String::new(),
                },
            )
            .await?;
        if response.session_token.is_empty() {
            return Err(DaemonHttpError::Malformed {
                method: "RefreshSession",
                reason: "it carried no access token".to_string(),
            });
        }
        Ok(response.session_token)
    }

    /// Every session this daemon knows of, its own and its peers'.
    pub(super) async fn list_sessions(
        &self,
        session_token: &str,
    ) -> Result<Vec<SessionEntry>, DaemonHttpError> {
        let response: ListSessionsResponse = self
            .unary(
                "session.SessionService",
                "ListSessions",
                ListSessionsRequest {
                    session_token: session_token.to_string(),
                },
            )
            .await?;
        Ok(response.sessions)
    }

    /// Ask the daemon to open this session's room with itself in it and the session's terminal
    /// exposed there.
    ///
    /// The response's coordinates are returned for a caller that wants them; a mirror joins
    /// `session-{id}`, which it derives itself, and wants the call's effect — the facilitating daemon
    /// in the room before anything looks for it there.
    pub(super) async fn open_session_room(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<OpenSessionRoomResponse, DaemonHttpError> {
        self.unary(
            "session_room.SessionRoomService",
            "OpenSessionRoom",
            OpenSessionRoomRequest {
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    async fn unary<Req: prost::Message, Res: prost::Message + Default>(
        &self,
        service: &str,
        method: &'static str,
        request: Req,
    ) -> Result<Res, DaemonHttpError> {
        let url = format!("{}/rpc/{service}/{method}", self.base_url);
        let response = self
            .http
            .post(&url)
            .header("content-type", "application/proto")
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| DaemonHttpError::Unreachable {
                url: url.clone(),
                reason: e.to_string(),
            })?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| DaemonHttpError::Unreachable {
                url,
                reason: format!("read the response body: {e}"),
            })?;

        if !status.is_success() {
            return Err(connect_error(method, status, &body));
        }
        Res::decode(&body[..]).map_err(|e| DaemonHttpError::Malformed {
            method,
            reason: e.to_string(),
        })
    }
}

/// Read a Connect error body — `{"code": "unauthenticated", "message": "…"}` — falling back to the
/// HTTP status when the daemon answered with something else entirely (a proxy's error page, say).
fn connect_error(
    method: &'static str,
    status: reqwest::StatusCode,
    body: &[u8],
) -> DaemonHttpError {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let field = |name: &str| -> Option<String> {
        parsed
            .as_ref()?
            .get(name)?
            .as_str()
            .map(std::string::ToString::to_string)
    };
    DaemonHttpError::Refused {
        method,
        code: field("code").unwrap_or_else(|| status.as_str().to_string()),
        message: field("message")
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string()),
    }
}
