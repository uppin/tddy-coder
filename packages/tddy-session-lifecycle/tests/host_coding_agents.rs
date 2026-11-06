//! A session host is constructed over the daemon's one coding-agent registry and launches from
//! that very instance — never a registry of its own, never an empty one in its place.
//!
//! Product: `docs/ft/daemon/1-WIP/PRD-2026-11-06-one-host-coding-agent-registry.md`.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{builtin_coding_agents, test_config};

#[test]
fn a_host_launches_from_the_registry_it_was_constructed_over() {
    // Given the daemon's registry
    let registry = builtin_coding_agents();

    // When a host is constructed over it
    let host = a_host_over(Arc::clone(&registry));

    // Then the host launches from that very instance
    assert!(
        Arc::ptr_eq(&host.launch_coding_agents(), &registry),
        "the host launches from a registry other than the one it was given"
    );
}

/// A host over `coding_agents` and nothing else of note.
fn a_host_over(coding_agents: Arc<HostCodingAgents>) -> DaemonSessionHost {
    let config = test_config();
    let base = PathBuf::from("/nonexistent-tddy-home");
    let sessions_base = base.clone();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        base,
        Arc::new(|_| None),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::CliSessionManager::new()),
        coding_agents,
    )
}
