//! Requests are routed to the daemon they address — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-forwarding-consumers.md`.

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::PeerRoute;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_host_service::peer_routing::PeerRouting;

/// One peer is eligible.
struct OnePeer;

#[async_trait::async_trait]
impl EligibleDaemonSource for OnePeer {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId("peer-daemon-1".to_string()),
            label: "peer".to_string(),
        }]
    }
}

fn this_daemon() -> DaemonConfig {
    DaemonConfig {
        daemon_instance_id: Some("this-daemon".to_string()),
        ..Default::default()
    }
}

#[test]
fn a_request_for_this_daemon_is_routed_locally_and_one_for_a_peer_is_forwarded() {
    // Given routing on this daemon, with one eligible peer
    let routing = PeerRouting::new(this_daemon(), Arc::new(OnePeer), None);

    // When a request for each is classified
    let routes = (
        routing.classify_daemon_route("this-daemon").unwrap(),
        routing.classify_daemon_route("peer-daemon-1").unwrap(),
    );

    // Then this daemon's is local and the peer's is forwarded to it
    assert_eq!(
        routes,
        (
            PeerRoute::Local,
            PeerRoute::Forward {
                peer_instance_id: "peer-daemon-1".to_string()
            }
        )
    );
}

#[test]
fn forwarding_without_a_common_room_is_a_failed_precondition_naming_the_rpc() {
    // Given routing with no common room
    let routing = PeerRouting::new(this_daemon(), Arc::new(OnePeer), None);

    // When a forward needs one
    let refused = routing
        .common_room_slot("AddProjectToHost")
        .err()
        .expect("there is no room to forward through");

    // Then the refusal says which RPC could not be forwarded
    assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        refused
            .message
            .starts_with("cannot forward AddProjectToHost:"),
        "got: {}",
        refused.message
    );
}
