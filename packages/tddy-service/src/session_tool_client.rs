//! Reaching a daemon from inside a session, over whichever transport that session actually has.
//!
//! Moved here from `tddy-tools` by `#unbundle` node 5, and this is the crate that should always have
//! owned it: every message it sends is defined by a proto in this package.
//!
//! # Why the move has a measurable outcome
//!
//! `tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin` all carried a **dev-dependency on
//! `tddy-tools`**, and it existed only for [`dispatch_via_sandbox_ipc`],
//! [`dispatch_session_tool`] and [`PASS_LONG_ENOUGH_TO_BE_SERVICE`]. With those three here, all
//! three crates drop that dependency entirely — asserted by a test in each, not described.
//!
//! # Nodes 6, 7 and 8 compile against this
//!
//! Node 7 serves the session-agent roster and agent conversations in front of this client, so its
//! shape is fixed here rather than in the node that consumes it.

use std::time::Duration;

/// How long a call may block before the caller should assume the far side is not a service.
///
/// Read by `tddy-daemon`'s roster acceptance suite, which is why it is public API rather than an
/// internal constant: the daemon asserts that a pass longer than this is treated as a dead peer.
pub const PASS_LONG_ENOUGH_TO_BE_SERVICE: Duration = Duration::from_secs(30);

/// The ceiling on how long a remote tool call may block.
///
/// Deliberately below `tddy-tool-engine`'s own 30s default: the far side must give up *after* this
/// side stops waiting, or a caller sees a timeout for a call that then succeeds invisibly.
pub const MAX_REMOTE_BLOCK_MS: u64 = 20_000;

/// Which transport a session has to its daemon.
///
/// Four variants rather than three because "a LiveKit session whose credentials are incomplete" is
/// not the same as "no LiveKit": the first is a misconfiguration to report, the second a routing
/// decision to make. Collapsing them turns a bad token into "this session has no daemon".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolTransport {
    /// A Unix socket into the jail, served by the sandbox runner.
    SandboxIpc { socket: String },
    /// Connect-HTTP to a daemon URL.
    DaemonHttp { url: String },
    /// A LiveKit room, with everything needed to join it.
    LiveKit {
        url: String,
        room: String,
        token: String,
    },
    /// LiveKit was indicated but the credentials are incomplete.
    IncompleteLiveKit { missing: Vec<String> },
}

impl SessionToolTransport {
    /// Detect the transport from the session's environment.
    pub fn from_env() -> Option<SessionToolTransport> {
        // TODO(tools-thinning): implement
        unimplemented!("SessionToolTransport::from_env")
    }
}

/// Why a session tool call could not be dispatched.
#[derive(Debug, thiserror::Error)]
pub enum SessionToolError {
    #[error("this session has no transport to a daemon")]
    NoTransport,
    #[error("LiveKit was indicated but {missing:?} are missing")]
    IncompleteLiveKit { missing: Vec<String> },
    #[error("the daemon refused the call: {reason}")]
    Refused { reason: String },
}

/// Dispatch a tool call over whichever transport this session has.
pub async fn dispatch_session_tool(
    _transport: &SessionToolTransport,
    _method: &str,
    _payload: &[u8],
) -> Result<Vec<u8>, SessionToolError> {
    // TODO(tools-thinning): implement
    unimplemented!("dispatch_session_tool")
}

/// Dispatch over the in-jail Unix socket specifically.
///
/// Kept as its own entry point because `tddy-daemon`'s and `tddy-sandbox-app`'s suites drive this
/// path directly — it is the one the sandbox runner's relay allowlist gates.
pub async fn dispatch_via_sandbox_ipc(
    _socket: &str,
    _method: &str,
    _payload: &[u8],
) -> Result<Vec<u8>, SessionToolError> {
    // TODO(tools-thinning): implement
    unimplemented!("dispatch_via_sandbox_ipc")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The far side must give up *after* this side stops waiting. If the remote ceiling were the
    /// higher of the two, a caller would see a timeout for a call that then succeeded invisibly —
    /// and the tool would have run twice on a retry.
    #[test]
    fn stops_waiting_before_the_tool_engine_does() {
        const TOOL_ENGINE_DEFAULT_MS: u64 = 30_000;
        assert!(
            MAX_REMOTE_BLOCK_MS < TOOL_ENGINE_DEFAULT_MS,
            "a remote call must not outlive the caller's patience"
        );
    }

    /// Incomplete LiveKit credentials are a misconfiguration to report, not an absence of transport
    /// to route around. Collapsing them turns a bad token into "this session has no daemon".
    #[test]
    fn tells_incomplete_livekit_apart_from_no_transport() {
        let incomplete = SessionToolError::IncompleteLiveKit {
            missing: vec!["TDDY_REMOTE_LIVEKIT_TOKEN".to_string()],
        };
        let absent = SessionToolError::NoTransport;

        assert!(incomplete.to_string().contains("missing"));
        assert!(absent.to_string().contains("no transport"));
    }

    #[tokio::test]
    async fn refuses_a_call_on_a_session_with_no_transport() {
        // Given
        let transport = SessionToolTransport::IncompleteLiveKit {
            missing: vec!["TDDY_REMOTE_LIVEKIT_URL".to_string()],
        };

        // When
        let outcome = dispatch_session_tool(&transport, "ExecuteTool", b"{}").await;

        // Then
        assert!(matches!(
            outcome,
            Err(SessionToolError::IncompleteLiveKit { .. })
        ));
    }
}
