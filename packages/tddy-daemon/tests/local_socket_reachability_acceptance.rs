//! The local socket keeps the surface `connection.ConnectionService` gave it.
//!
//! That service carried all 90 methods on the daemon's local Unix socket, so any local caller could
//! reach any family — `tddy-sandbox-app` dials it today, and nothing constrains callers outside this
//! repo. **Dropping a family from the socket is a silent capability removal on a privileged
//! interface**, and its failure mode is a caller that used to work receiving `unimplemented` with no
//! announcement.
//!
//! `ExecuteTool` is the sharpest case: it is what `tddy-sandbox-runner`'s relay allowlist gates at
//! `runner.rs:69` and what an agent inside a jail calls, so a family that silently left the socket
//! would fail there rather than anywhere a developer is looking.
//!
//! **Node 4 is already inconsistent with the policy** — it dropped family T, so there is no
//! `livekit_tonic_adapter.rs`. That gap is a predecessor's file to close and is recorded in
//! `docs/dev/todo/2026-09-10-family-t-was-dropped-from-the-local-socket-before-the-policy-existed.md`.

use std::path::{Path, PathBuf};

fn daemon_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn local_socket_server() -> String {
    std::fs::read_to_string(daemon_src().join("local_socket_server.rs"))
        .expect("local_socket_server.rs is readable")
}

/// Node 7's two services and node 8's three, all mounted on the one `Server::builder()`.
///
/// Both nodes' services are asserted here because the file is shared: node 7 adds two `add_service`
/// lines and node 8 adds three, and a cascade that resolved the conflict by keeping only one side
/// would leave the other silently unreachable. Asserting both is what catches that.
#[test]
fn the_socket_serves_every_service_the_stack_moved_onto_it() {
    // Given
    let server = local_socket_server();

    // Then
    for expected in [
        // node 7
        "SessionAgentServiceServer",
        "ActivityServiceServer",
        // node 8
        "CatalogServiceServer",
        "ExecToolServiceServer",
        "PrStackServiceServer",
    ] {
        assert!(
            server.contains(expected),
            "{expected} is not mounted on the local socket; a caller that reached it through \
             connection.ConnectionService would silently stop being able to"
        );
    }
}

/// The adapters are **generated**. A gap in node 6's generator is reported upward, not worked around
/// with 16 `async fn`s — hand-writing them is the cost the generator exists to remove.
///
/// Asserted as an absence of *these nodes'* adapters rather than against an exact file list, so the
/// test says the same thing whether or not the branch has caught up with node 1's two.
#[test]
fn nodes_seven_and_eight_add_no_hand_written_adapter() {
    for name in [
        "session_agents_tonic_adapter.rs",
        "activity_tonic_adapter.rs",
        "catalog_tonic_adapter.rs",
        "exec_tools_tonic_adapter.rs",
        "pr_stack_tonic_adapter.rs",
    ] {
        assert!(
            !daemon_src().join(name).exists(),
            "{name} is hand-written; these services must use node 6's generator"
        );
    }
}
