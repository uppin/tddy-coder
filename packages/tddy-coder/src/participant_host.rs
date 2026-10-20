//! Serving the coder's RPC surface as a room participant, as a port.
//!
//! The coder hosts its surface in a room — once with a token it was handed, or with reconnect and a
//! token minted from API credentials — publishes its participant metadata from a `watch` channel,
//! and may watch the codex-OAuth file. The library asks for that through
//! [`SessionParticipantHost`]; the binary supplies the implementation.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// How the participant proves it may join — and so which room it joins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParticipantAuth {
    /// Mint a token for `room` from API credentials, valid for `ttl` (re-minted on reconnect).
    Credentials {
        api_key: String,
        api_secret: String,
        room: String,
        ttl: Duration,
    },
    /// A token minted elsewhere; the room is the one it was minted for.
    Token(String),
}

/// Where the participant serves, as whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantJoin {
    pub url: String,
    pub identity: String,
    pub auth: ParticipantAuth,
}

/// How the participant behaves while it serves.
pub struct ParticipantOptions {
    pub metadata: Option<tokio::sync::watch::Receiver<String>>,
    pub codex_oauth_watch: Option<PathBuf>,
    pub projects_registry_dir: Option<PathBuf>,
    pub reconnect: bool,
    pub shutdown: Arc<AtomicBool>,
}

/// The participant's task; it ends when the room does, or on shutdown. Dropping it does not stop
/// the participant.
pub struct ServedParticipant {
    serving: tokio::task::JoinHandle<()>,
}

impl ServedParticipant {
    pub fn new(serving: tokio::task::JoinHandle<()>) -> Self {
        Self { serving }
    }

    /// Resolves when the participant's task has ended.
    pub async fn ended(self) {
        if let Err(e) = self.serving.await {
            log::error!(target: "tddy_coder::participant_host", "participant task failed: {e}");
        }
    }
}

/// Hosts an RPC surface as a room participant.
#[async_trait::async_trait]
pub trait SessionParticipantHost: Send + Sync {
    async fn serve(
        &self,
        join: ParticipantJoin,
        surface: tddy_rpc::MultiRpcService,
        options: ParticipantOptions,
    ) -> Result<ServedParticipant, String>;

    /// A provider that mints room tokens for other participants from API credentials, for the
    /// token service the coder serves next to its surface. A host with no room transport refuses.
    fn token_provider(
        &self,
        api_key: &str,
        api_secret: &str,
        ttl: Duration,
    ) -> Result<Arc<dyn tddy_service::TokenProvider>, String> {
        let _ = (api_key, api_secret, ttl);
        Err("no room transport is wired into this build; cannot mint room tokens".to_string())
    }
}

/// The host as the run functions share it.
pub type ParticipantHost = Arc<dyn SessionParticipantHost>;

/// [`SessionParticipantHost::serve`], then waits for the participant to end. A refusal is logged,
/// not returned: the coder keeps running its workflow without the room.
pub async fn serve_until_ended(
    host: ParticipantHost,
    join: ParticipantJoin,
    surface: tddy_rpc::MultiRpcService,
    options: ParticipantOptions,
) {
    match host.serve(join, surface, options).await {
        Ok(served) => served.ended().await,
        Err(e) => {
            log::error!(target: "tddy_coder::participant_host", "LiveKit connect failed: {e}")
        }
    }
}

/// How long a token minted from API credentials stays valid.
pub const PARTICIPANT_TOKEN_TTL: Duration = Duration::from_secs(6 * 3600);

/// The join and reconnect policy the coder's LiveKit flags ask for: with API credentials it
/// re-mints for `room` and reconnects; with a token it serves once, with that token.
pub fn participant_join_from_cli(
    url: &str,
    room: &str,
    identity: &str,
    token: Option<&str>,
    credentials: Option<(&str, &str)>,
) -> (ParticipantJoin, bool) {
    let auth = match (credentials, token) {
        (Some((api_key, api_secret)), _) => ParticipantAuth::Credentials {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
            room: room.to_string(),
            ttl: PARTICIPANT_TOKEN_TTL,
        },
        (None, Some(token)) => ParticipantAuth::Token(token.to_string()),
        (None, None) => unreachable!(
            "validate_livekit_args requires a token or API credentials before a room is joined"
        ),
    };
    let reconnect = matches!(auth, ParticipantAuth::Credentials { .. });
    (
        ParticipantJoin {
            url: url.to_string(),
            identity: identity.to_string(),
            auth,
        },
        reconnect,
    )
}

/// No transport wired: the coder serves nothing in any room.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoParticipantHost;

#[async_trait::async_trait]
impl SessionParticipantHost for NoParticipantHost {
    async fn serve(
        &self,
        join: ParticipantJoin,
        _surface: tddy_rpc::MultiRpcService,
        _options: ParticipantOptions,
    ) -> Result<ServedParticipant, String> {
        Err(format!(
            "no room transport is wired into this build; cannot serve {} at {}",
            join.identity, join.url
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_nothing_wired_serving_is_refused_naming_the_identity_and_url() {
        let refused = NoParticipantHost
            .serve(
                ParticipantJoin {
                    url: "ws://10.0.0.5:7880".to_string(),
                    identity: "session-host-abc".to_string(),
                    auth: ParticipantAuth::Token("t".to_string()),
                },
                tddy_rpc::MultiRpcService::new(Vec::new()),
                ParticipantOptions {
                    metadata: None,
                    codex_oauth_watch: None,
                    projects_registry_dir: None,
                    reconnect: false,
                    shutdown: Arc::new(AtomicBool::new(false)),
                },
            )
            .await
            .err()
            .expect("nothing can host the participant");

        assert_eq!(
            refused,
            "no room transport is wired into this build; cannot serve session-host-abc at ws://10.0.0.5:7880"
        );
    }
}
