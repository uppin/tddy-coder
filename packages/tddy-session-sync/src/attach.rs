//! Resolving the session, and joining the room it is broadcast in.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md` § Client — attaching to a session
//! (AC21-AC24).
//!
//! Two legs, in this order, and the order is forced rather than chosen:
//!
//! 1. **Connect-HTTP against `--daemon-url`** — exchange a refresh token if that is what was given,
//!    then `ListSessions` to learn the session's `project_id`, worktree path and
//!    **`daemon_instance_id`**.
//! 2. **LiveKit** — mint a token for `session-{session_id}`, join it, and address the RPC client at
//!    `daemon-{daemon_instance_id}`.
//!
//! The `daemon_instance_id` is what makes the order forced: a room's RPC client must be addressed
//! at a participant identity, that identity is `daemon-{instance_id}`, and the instance id is one
//! of the very things being resolved. Resolving it over the room would be circular. The HTTP leg is
//! the same surface `tddy-web` lists sessions on and the same one `tddy-remote-git-repo` exchanges
//! tokens on, so nothing new is invented to break the cycle — and a session that does not exist is
//! refused before a room is joined for it.
//!
//! Everything decidable without I/O — which room, which identity, which entry of a session list,
//! and what makes an entry unusable — is a pure function below, so it is testable without a daemon
//! and without a LiveKit server.

use std::time::Duration;

use tddy_livekit::broadcast::BroadcastMessage;
use tddy_livekit::client_connect::{connect_client, ConnectError, ConnectedClient};
use tddy_livekit::{BroadcastChannel, TokenGenerator, DEFAULT_LIVEKIT_JWT_TTL_SECS};
use tddy_service::proto::auth::{RefreshSessionRequest, RefreshSessionResponse};
use tddy_service::proto::connection::{ListSessionsRequest, ListSessionsResponse, SessionEntry};
use tddy_service::session_activity::SESSION_ACTIVITY_TOPIC;
use tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC;
use tokio::sync::mpsc;

use crate::credentials::{Credentials, DaemonToken};

/// The room a session is broadcast in. One room per session, named after it — see
/// `docs/ft/daemon/session-room.md`.
pub const SESSION_ROOM_PREFIX: &str = "session-";

/// The identity prefix `tddy-service`'s token service reserves for daemons
/// ([`tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX`]). A client that minted itself an identity
/// under it would be addressable as a daemon by every peer in the room.
const DAEMON_IDENTITY_PREFIX: &str = "daemon-";

/// The identity prefix this client joins under. Deliberately its own word rather than a bare
/// `sync-`: an identity is what a daemon operator sees in the room roster, and "which program is
/// this" is the first thing they need from it.
const SYNCER_IDENTITY_PREFIX: &str = "session-sync-";

/// A `workspace` session has no facilitating daemon and therefore no room at all (see
/// `docs/ft/daemon/session-room.md`), so there is nothing for this client to join.
const WORKSPACE_SESSION_TYPE: &str = "workspace";

/// Where a session lives, resolved rather than configured.
///
/// AC21: these are **not** flags. A user who had to spell out the project and the daemon holding a
/// session could spell them wrong, and a mirror built against the wrong worktree is wrong in a way
/// nothing downstream can detect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAddress {
    pub session_id: String,
    pub project_id: String,
    /// The session's checkout on the daemon host. Carried because the file-read RPCs address by it;
    /// nothing local is resolved against it.
    pub worktree_path: String,
    pub daemon_instance_id: String,
}

/// The room, the client addressed at the daemon in it, and the two topics the syncer consumes.
///
/// The whole [`ConnectedClient`] is carried rather than only its `client`, because the connection
/// lives exactly as long as its room handle: dropping it leaves the room, and a syncer that left
/// the room receives no further broadcast while reporting no failure at all.
pub struct AttachedSession {
    pub address: SessionAddress,
    /// The access token every RPC carries, already exchanged if a refresh token was what was given.
    pub session_token: String,
    pub connection: ConnectedClient,
    pub session_activity: mpsc::UnboundedReceiver<BroadcastMessage>,
    pub worktree_activity: mpsc::UnboundedReceiver<BroadcastMessage>,
}

/// Why the syncer could not attach to the session it was asked for.
///
/// Every variant names the session id or the identity at fault. A client that fails to attach has
/// produced no mirror at all, so its message is the entire diagnostic its user gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    /// The daemon knows no session by that id. A hard error naming it, never a default (AC21).
    SessionNotFound { session_id: String },
    /// The session exists and has no room to join — a `workspace` session.
    SessionHasNoRoom {
        session_id: String,
        session_type: String,
    },
    /// The session was listed without something the syncer needs. Reported rather than filled in:
    /// a fabricated project or daemon id addresses somebody else's repository.
    SessionIncomplete {
        session_id: String,
        field: &'static str,
    },
    /// A call to the daemon's HTTP surface did not produce an answer.
    Daemon(DaemonHttpError),
    /// The LiveKit token could not be minted from the configured key and secret.
    Token { room: String, reason: String },
    /// The room itself could not be joined.
    Room {
        url: String,
        room: String,
        reason: String,
    },
    /// The room was joined and the facilitating daemon is not in it. Rooms are not re-opened when a
    /// daemon restarts, so this is reported rather than waited out.
    DaemonAbsent {
        identity: String,
        room: String,
        waited: Duration,
    },
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::SessionNotFound { session_id } => {
                write!(f, "no session \"{session_id}\" on this daemon or its peers")
            }
            AttachError::SessionHasNoRoom {
                session_id,
                session_type,
            } => write!(
                f,
                "session \"{session_id}\" is a {session_type} session, which has no facilitating \
                 daemon and no room to watch"
            ),
            AttachError::SessionIncomplete { session_id, field } => write!(
                f,
                "session \"{session_id}\" was listed without a {field}, so there is nothing to \
                 mirror it from"
            ),
            AttachError::Daemon(e) => write!(f, "{e}"),
            AttachError::Token { room, reason } => write!(
                f,
                "could not mint a LiveKit token for room \"{room}\": {reason}"
            ),
            AttachError::Room { url, room, reason } => {
                write!(f, "could not join room \"{room}\" at {url}: {reason}")
            }
            AttachError::DaemonAbsent {
                identity,
                room,
                waited,
            } => write!(
                f,
                "daemon \"{identity}\" is not in room \"{room}\" after {}s; the session's room is \
                 not open (a daemon that restarted does not re-open it)",
                waited.as_secs()
            ),
        }
    }
}

impl std::error::Error for AttachError {}

impl From<DaemonHttpError> for AttachError {
    fn from(e: DaemonHttpError) -> Self {
        AttachError::Daemon(e)
    }
}

/// The room a session's signals are broadcast in.
pub fn session_room_name(session_id: &str) -> String {
    format!("{SESSION_ROOM_PREFIX}{session_id}")
}

/// The participant identity the facilitating daemon serves its RPCs under.
pub fn daemon_identity(daemon_instance_id: &str) -> String {
    format!("{DAEMON_IDENTITY_PREFIX}{daemon_instance_id}")
}

/// The identity this client joins under.
///
/// Two properties, both load-bearing. It is **not** under [`DAEMON_IDENTITY_PREFIX`], which the
/// token service reserves — an identity there is how every peer in the system addresses a daemon,
/// and a client wearing one would be sent calls it cannot serve. And it carries a `nonce`, because
/// LiveKit disconnects the older participant when a second one joins with the same identity: two
/// developers mirroring one session would otherwise take turns evicting each other, each seeing
/// only a room that keeps dropping.
pub fn syncer_identity(session_id: &str, nonce: &str) -> String {
    format!("{SYNCER_IDENTITY_PREFIX}{session_id}-{nonce}")
}

/// Find the session in what the daemon listed, or say precisely why it cannot be mirrored.
///
/// Never falls back to "the only session" or "the newest one": the id is the whole request, and
/// mirroring a different session than the one asked for is a failure nothing downstream detects.
pub fn resolve_session(
    sessions: &[SessionEntry],
    session_id: &str,
) -> Result<SessionAddress, AttachError> {
    let entry = sessions
        .iter()
        .find(|entry| entry.session_id == session_id)
        .ok_or_else(|| AttachError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;

    if entry.session_type == WORKSPACE_SESSION_TYPE {
        return Err(AttachError::SessionHasNoRoom {
            session_id: session_id.to_string(),
            session_type: entry.session_type.clone(),
        });
    }

    // Each of these is something the syncer would otherwise have to invent: the project a clone
    // resolves against, the daemon a room is addressed at, and the checkout the file reads name.
    // An empty one is reported here, where it can name the session, rather than surfacing later as
    // a clone of `":"` or a wait for participant `"daemon-"`.
    for (field, value) in [
        ("project_id", &entry.project_id),
        ("daemon_instance_id", &entry.daemon_instance_id),
        ("worktree path", &entry.repo_path),
    ] {
        if value.is_empty() {
            return Err(AttachError::SessionIncomplete {
                session_id: session_id.to_string(),
                field,
            });
        }
    }

    Ok(SessionAddress {
        session_id: entry.session_id.clone(),
        project_id: entry.project_id.clone(),
        worktree_path: entry.repo_path.clone(),
        daemon_instance_id: entry.daemon_instance_id.clone(),
    })
}

/// Resolve the session over HTTP, then join its room and subscribe to both broadcast topics.
pub async fn attach(credentials: &Credentials) -> Result<AttachedSession, AttachError> {
    let http = DaemonHttp::new(&credentials.daemon_url, credentials.connect_timeout)?;

    // One access token serves every leg from here on. A refresh token is exchanged first because
    // nothing downstream accepts one: the RPCs carry a 5-minute access token and nothing else.
    let session_token = match &credentials.token {
        DaemonToken::Access(token) => token.clone(),
        DaemonToken::Refresh(token) => http.refresh_session(token).await?,
    };

    let listed = http.list_sessions(&session_token).await?;
    let address = resolve_session(&listed, &credentials.session_id)?;

    let room_name = session_room_name(&address.session_id);
    let identity = syncer_identity(&address.session_id, &join_nonce());
    let token = TokenGenerator::new(
        credentials.livekit.api_key.clone(),
        credentials.livekit.api_secret.clone(),
        room_name.clone(),
        identity,
        Duration::from_secs(DEFAULT_LIVEKIT_JWT_TTL_SECS),
    )
    .generate()
    .map_err(|e| AttachError::Token {
        room: room_name.clone(),
        reason: e.to_string(),
    })?;

    let daemon = daemon_identity(&address.daemon_instance_id);
    let connected = connect_client(
        &credentials.livekit.url,
        &token,
        &daemon,
        credentials.connect_timeout,
    )
    .await
    .map_err(|e| match e {
        ConnectError::Room(reason) => AttachError::Room {
            url: credentials.livekit.url.clone(),
            room: room_name.clone(),
            reason,
        },
        ConnectError::ParticipantAbsent { identity, waited } => AttachError::DaemonAbsent {
            identity,
            room: room_name.clone(),
            waited,
        },
    })?;

    // Subscribed before the first RPC is made, so a record published while the syncer was still
    // resolving the session is delivered rather than missed. Each subscription owns its own event
    // stream, so the two topics and the RPC loop never race for one event.
    let session_activity =
        BroadcastChannel::new(connected.room.clone(), SESSION_ACTIVITY_TOPIC).subscribe();
    let worktree_activity =
        BroadcastChannel::new(connected.room.clone(), WORKTREE_ACTIVITY_TOPIC).subscribe();

    Ok(AttachedSession {
        address,
        session_token,
        connection: connected,
        session_activity,
        worktree_activity,
    })
}

/// What distinguishes this join from another client's join of the same room.
///
/// The process id alone would not: pid numbers are small and reused, so two developers mirroring
/// one session from two hosts can hold the same one. The wall clock alone would not either — two
/// syncers started by one script share a millisecond. Together they are distinctive in the only
/// way that matters, which is that a second joiner does not evict the first.
fn join_nonce() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    format!("{millis}-{}", std::process::id())
}

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
struct DaemonHttp {
    http: reqwest::Client,
    base_url: String,
}

impl DaemonHttp {
    fn new(base_url: &str, timeout: Duration) -> Result<Self, DaemonHttpError> {
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
    async fn refresh_session(&self, refresh_token: &str) -> Result<String, DaemonHttpError> {
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
            return Err(DaemonHttpError::Malformed {
                method: "RefreshSession",
                reason: "it carried no access token".to_string(),
            });
        }
        Ok(response.session_token)
    }

    /// Every session this daemon knows of, its own and its peers'.
    async fn list_sessions(
        &self,
        session_token: &str,
    ) -> Result<Vec<SessionEntry>, DaemonHttpError> {
        let response: ListSessionsResponse = self
            .unary(
                "connection.ConnectionService",
                "ListSessions",
                ListSessionsRequest {
                    session_token: session_token.to_string(),
                },
            )
            .await?;
        Ok(response.sessions)
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
