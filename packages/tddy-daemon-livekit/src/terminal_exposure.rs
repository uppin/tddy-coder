//! LiveKit's [`TerminalExposure`]: a session's PTY served as a participant in a room.
//!
//! Joins `at.room` as `at.identity` with a token it mints from the configured credentials, serves
//! the terminal's RPC surface there, and republishes the session's metadata block on an interval so
//! a rejoin or a dropped `set_metadata` does not lose the session's cross-host association.

use std::sync::Arc;

use tddy_daemon_extensions::{ExposedTerminal, TerminalAddress, TerminalExposure};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_livekit::{LiveKitParticipant, TokenGenerator};
use tddy_rpc::Status;
use tokio::sync::watch;

/// How long a participant token minted for an exposed terminal stays valid.
const PARTICIPANT_TOKEN_TTL: std::time::Duration = std::time::Duration::from_secs(86400);

/// How often an exposed terminal re-publishes its `session` participant block.
///
/// Matches the owned-project-count poller's 30s cadence in `tddy-livekit`, and for the same reason:
/// it is a bounded refresh of a value the room must not silently lose, not a live feed. Nothing in
/// the block changes between ticks — what changes is whether the last publish landed.
const SESSION_METADATA_REPUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Serves terminals in this daemon's LiveKit rooms.
pub struct LiveKitTerminalExposure {
    url: String,
    api_key: String,
    api_secret: String,
}

impl LiveKitTerminalExposure {
    /// The exposure the `livekit:` block supports, or `None` without credentials.
    ///
    /// Joins at the block's `public_url` when it has one, as the terminal bridge always has: the
    /// participant is a client of the server, not the daemon's own loopback.
    pub fn from_config(config: &DaemonConfig) -> Option<Arc<Self>> {
        let livekit = config.livekit.as_ref()?;
        let url = livekit.url.as_ref()?;
        Some(Arc::new(Self {
            url: livekit.public_url.as_ref().unwrap_or(url).clone(),
            api_key: livekit.api_key.as_ref()?.clone(),
            api_secret: livekit.api_secret.as_ref()?.clone(),
        }))
    }
}

#[async_trait::async_trait]
impl TerminalExposure for LiveKitTerminalExposure {
    /// A session whose agent is still running keeps its block current for as long as the room
    /// lasts: published at join and re-published unchanged on
    /// [`SESSION_METADATA_REPUBLISH_INTERVAL`], because a value published once is a value a single
    /// failed `set_metadata` — or a room rejoin — loses for the life of the session. It is what a
    /// PR-Stack view on **another host** hydrates a row from; `ListSessions` does not fan out.
    async fn expose(
        &self,
        at: &TerminalAddress,
        terminal: Arc<dyn tddy_rpc::RpcService>,
        metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        let token = TokenGenerator::new(
            self.api_key.clone(),
            self.api_secret.clone(),
            at.room.clone(),
            at.identity.clone(),
            PARTICIPANT_TOKEN_TTL,
        )
        .generate()
        .map_err(|e| Status::internal(format!("token generate: {e}")))?;

        let participant = LiveKitParticipant::connect(
            &self.url,
            &token,
            terminal,
            Default::default(),
            None,
            None,
        )
        .await
        .map_err(|e| Status::internal(format!("LiveKitParticipant::connect: {e}")))?;

        let identity = at.identity.clone();
        // Taken before the event loop consumes the participant, and shared with the participant's
        // own metadata publishers so two writers never race a `set_metadata` past each other.
        let local = participant.room().local_participant().clone();
        let publish_lock = participant.metadata_publish_lock();
        let serving = tokio::spawn(async move {
            // The watcher publishes on *change*, so the channel starts empty and the block is sent
            // after the receiver exists: a value put into a channel before anyone subscribed is
            // the subscriber's already-seen initial value and would never reach the room.
            let republishing = metadata_json.map(|json| {
                let (tx, rx) = watch::channel(String::new());
                tddy_livekit::spawn_local_participant_metadata_watcher(rx, local, publish_lock);
                tokio::spawn(async move {
                    let mut ticks = tokio::time::interval(SESSION_METADATA_REPUBLISH_INTERVAL);
                    loop {
                        // The first tick completes immediately, so the block reaches the room at
                        // join and every interval thereafter.
                        ticks.tick().await;
                        // `watch::Sender::send` marks the value changed whether or not it differs,
                        // so each tick is a real publish. It fails only once the watcher is gone,
                        // which is the end of this publisher too.
                        if tx.send(json.clone()).is_err() {
                            break;
                        }
                    }
                })
            });
            participant.run().await;
            // The room is gone, so there is nothing left to advertise to.
            if let Some(republishing) = republishing {
                republishing.abort();
            }
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "LiveKit terminal exposure ended for identity {identity}"
            );
        });
        Ok(ExposedTerminal::new(serving))
    }
}
