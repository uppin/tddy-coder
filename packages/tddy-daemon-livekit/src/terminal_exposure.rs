//! LiveKit's [`TerminalExposure`]: a session's PTY served as a participant in a room.
//!
//! Joins `at.room` as `at.identity` with a token it mints from the configured credentials, serves
//! the terminal's RPC surface there, and republishes the session's metadata block on an interval so
//! a rejoin or a dropped `set_metadata` does not lose the session's cross-host association.

use std::sync::Arc;

use tddy_daemon_extensions::{ExposedTerminal, TerminalAddress, TerminalExposure};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

/// Serves terminals in this daemon's LiveKit rooms.
pub struct LiveKitTerminalExposure {
    url: String,
    api_key: String,
    api_secret: String,
}

impl LiveKitTerminalExposure {
    /// The exposure the `livekit:` block supports, or `None` without credentials.
    pub fn from_config(config: &DaemonConfig) -> Option<Arc<Self>> {
        let _ = config;
        todo!("LiveKitTerminalExposure::from_config")
    }
}

#[async_trait::async_trait]
impl TerminalExposure for LiveKitTerminalExposure {
    async fn expose(
        &self,
        at: &TerminalAddress,
        terminal: Arc<dyn tddy_rpc::RpcService>,
        metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        let _ = (at, terminal, metadata_json, &self.url, &self.api_key, &self.api_secret);
        todo!("LiveKitTerminalExposure::expose")
    }
}
