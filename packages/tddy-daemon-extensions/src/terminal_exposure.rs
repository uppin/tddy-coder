//! Making a session's terminal drivable from outside this host, as a port.
//!
//! The session manager owns which terminals may be exposed and makes each exposure single-flight;
//! the integration owns how — joining a room, minting the participant's token, advertising the
//! session's metadata block. A daemon with nothing wired has [`NoTerminalExposure`]: its terminals
//! are reachable over its own connection only, as on a desktop.

use std::sync::Arc;

use tddy_rpc::Status;

/// Where an exposed terminal is served: a room, under an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalAddress {
    pub room: String,
    pub identity: String,
}

/// A running exposure. It ends when the room or the terminal does.
#[derive(Debug)]
pub struct ExposedTerminal {
    serving: tokio::task::JoinHandle<()>,
}

impl ExposedTerminal {
    pub fn new(serving: tokio::task::JoinHandle<()>) -> Self {
        Self { serving }
    }

    /// Whether the exposure has ended — a finished one is a terminal no longer served.
    pub fn is_finished(&self) -> bool {
        self.serving.is_finished()
    }
}

/// Serves terminals to participants outside this host.
#[async_trait::async_trait]
pub trait TerminalExposure: Send + Sync {
    /// Serve `terminal` — the RPC surface driving one PTY — under `at.identity` in `at.room`,
    /// advertising `metadata_json` while it is served.
    async fn expose(
        &self,
        at: &TerminalAddress,
        terminal: Arc<dyn tddy_rpc::RpcService>,
        metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status>;
}

/// Nothing to expose through: a failed precondition, which callers read as "not exposed".
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTerminalExposure;

#[async_trait::async_trait]
impl TerminalExposure for NoTerminalExposure {
    async fn expose(
        &self,
        _at: &TerminalAddress,
        _terminal: Arc<dyn tddy_rpc::RpcService>,
        _metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        Err(Status::failed_precondition(
            "no terminal exposure is wired: this daemon's terminals are reachable over its own \
             connection only",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_nothing_wired_exposing_is_a_failed_precondition() {
        let refused = NoTerminalExposure
            .expose(
                &TerminalAddress {
                    room: "tddy-lobby".to_string(),
                    identity: "daemon-host-1780828020298-abc".to_string(),
                },
                Arc::new(tddy_rpc::MultiRpcService::new(Vec::new())),
                None,
            )
            .await
            .expect_err("nothing can serve the terminal");

        assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
    }
}
