//! A split agent is spawned with the room join the split-room port issued — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-split-transport.md`.

use pretty_assertions::assert_eq;
use tddy_daemon_extensions::SplitAgentJoin;
use tddy_session_split::split_session::{split_remote_tool_env_from_join, SplitSpawnTarget};

#[test]
fn a_split_agent_is_spawned_with_the_join_the_port_issued() {
    // Given the join the port issued for the split session's room
    let join = SplitAgentJoin {
        url: "ws://10.0.0.5:7880".to_string(),
        room: "session-1780828020298-abc".to_string(),
        token: "the-agent-join-token".to_string(),
    };
    let target = SplitSpawnTarget {
        session_id: "1780828020298-abc",
        codebase_instance_id: "codebase-host",
        codebase_session_id: "1780828020298-ws",
        session_token: "the-caller-token",
    };

    // When the agent's tool environment is built from it
    let env = split_remote_tool_env_from_join(
        &join,
        "daemon-facilitating-host",
        "the-agent-session-token".to_string(),
        &target,
    );

    // Then the agent reaches the codebase session through that room, with that join and identity
    assert_eq!(
        (
            env.session_id.as_str(),
            env.daemon_instance_id.as_deref(),
            env.session_token.as_str(),
            env.livekit_url.as_deref(),
            env.livekit_room.as_deref(),
            env.livekit_token.as_deref(),
            env.server_identity.as_deref(),
            env.daemon_url.as_str(),
        ),
        (
            "1780828020298-ws",
            Some("codebase-host"),
            "the-agent-session-token",
            Some("ws://10.0.0.5:7880"),
            Some("session-1780828020298-abc"),
            Some("the-agent-join-token"),
            Some("daemon-facilitating-host"),
            "",
        )
    );
}
