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

/// The participant's task; it ends when the room does, or on shutdown.
pub struct ServedParticipant {
    _serving: tokio::task::JoinHandle<()>,
}

impl ServedParticipant {
    pub fn new(serving: tokio::task::JoinHandle<()>) -> Self {
        Self { _serving: serving }
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
}

/// No transport wired: the coder serves nothing in any room.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoParticipantHost;

#[async_trait::async_trait]
impl SessionParticipantHost for NoParticipantHost {
    async fn serve(
        &self,
        _join: ParticipantJoin,
        _surface: tddy_rpc::MultiRpcService,
        _options: ParticipantOptions,
    ) -> Result<ServedParticipant, String> {
        todo!("NoParticipantHost::serve")
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
