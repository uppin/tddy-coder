//! The local socket keeps the surface `connection.ConnectionService` gave it.
//!
//! That service carried all 90 methods on the daemon's local Unix socket, so any local caller could
//! reach any family — `tddy-sandbox-app` dials it today, and nothing constrains callers outside this
//! repo. **Dropping a family from the socket is a silent capability removal on a privileged
//! interface**, and its failure mode is a caller that used to work receiving `unimplemented` with no
//! announcement.
//!
//! So every family that moves stays reachable there. This suite is the check, and it is written
//! against the socket's registered services rather than against a changeset claim.
//!
//! **Node 4 is already inconsistent with the policy** — it dropped family T, so there is no
//! `livekit_tonic_adapter.rs` and the socket still serves three services. That gap is a
//! predecessor's file to close and is recorded in `docs/dev/todo/`; this suite asserts only what
//! node 7 owns.

use std::path::{Path, PathBuf};

fn daemon_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn local_socket_server() -> String {
    std::fs::read_to_string(daemon_src().join("local_socket_server.rs"))
        .expect("local_socket_server.rs is readable")
}

/// Node 7's two services must be mounted on the one `Server::builder()` the socket uses.
#[test]
fn the_socket_serves_the_session_agent_and_activity_services() {
    // Given
    let server = local_socket_server();

    // Then
    for expected in ["SessionAgentServiceServer", "ActivityServiceServer"] {
        assert!(
            server.contains(expected),
            "{expected} is not mounted on the local socket; family B or M/N would silently stop \
             being reachable for a caller that used to reach it through connection.ConnectionService"
        );
    }
}

/// The adapters are **generated**. If the generator cannot produce one, that is a gap in node 6 to
/// report upward — not a licence to hand-write 17 `async fn`s here, which is the cost the generator
/// exists to remove.
///
/// Asserted as an absence of *this node's* adapters rather than against an exact file list, so the
/// test says the same thing whether or not the branch has caught up with node 1's two.
#[test]
fn this_node_adds_no_hand_written_adapter() {
    // Given
    let forbidden = [
        "session_agents_tonic_adapter.rs",
        "activity_tonic_adapter.rs",
        "session_agent_tonic_adapter.rs",
    ];

    // Then
    for name in forbidden {
        assert!(
            !daemon_src().join(name).exists(),
            "{name} is hand-written; node 7's services must use node 6's generator"
        );
    }
}
