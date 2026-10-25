//! In remote-codebase mode `tddy-tools` reaches its daemon through the local relay's HTTP `/rpc`
//! (`TDDY_REMOTE_DAEMON_URL`), and the roster and conversation RPCs must travel that way too.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host

use serial_test::serial;
use tddy_tools::session_agents::AgentConversationLink;

struct RemoteCodebaseEnv;

impl RemoteCodebaseEnv {
    /// The variables `tddy-tools --mcp` is given in remote-codebase mode, and only those.
    fn set() -> Self {
        for var in [
            "TDDY_SANDBOX_TOOL_IPC",
            "TDDY_REMOTE_DAEMON_SOCKET",
            "TDDY_REMOTE_LIVEKIT_URL",
            "TDDY_REMOTE_LIVEKIT_ROOM",
            "TDDY_REMOTE_LIVEKIT_TOKEN",
            "TDDY_REMOTE_LIVEKIT_SERVER_IDENTITY",
        ] {
            std::env::remove_var(var);
        }
        std::env::set_var("TDDY_REMOTE_SESSION_ID", "ws-session-1");
        std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:9/rpc");
        std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", "tok");
        std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", "codebase-host");
        Self
    }
}

impl Drop for RemoteCodebaseEnv {
    fn drop(&mut self) {
        for var in [
            "TDDY_REMOTE_SESSION_ID",
            "TDDY_REMOTE_DAEMON_URL",
            "TDDY_REMOTE_SESSION_TOKEN",
            "TDDY_REMOTE_DAEMON_INSTANCE_ID",
        ] {
            std::env::remove_var(var);
        }
    }
}

#[tokio::test]
#[serial]
async fn the_relay_transport_carries_the_conversation_rpcs() {
    // Given
    let _env = RemoteCodebaseEnv::set();

    // When
    let link = AgentConversationLink::connect().await;

    // Then
    assert!(
        link.is_ok(),
        "remote-codebase mode reaches conversations through the relay: {:?}",
        link.err()
    );
}
