//! Acceptance: `PropagateWorkItem` and `LandWorkQueue` over the daemon's RPCs (`#workqueue` 14/14).
//!
//! The git and GitHub behaviour is proved through the tools in `work_item_git_tools_acceptance` (the
//! RPCs run the same engines); here the wire: what is refused before anything is touched.

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    LandWorkQueueRequest, PropagateWorkItemRequest, WorkQueueAddress, WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-land";

fn a_host_owning_a_queue() -> (Host, WorkQueueAddress) {
    let host = a_host(HOST_A, &Arc::new(InProcessPeers::default()));
    SessionWorkQueues::new(host.sessions.path().join("sessions"))
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    let address = WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: OWNER.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    };
    (host, address)
}

#[tokio::test]
async fn propagating_without_naming_an_item_is_refused() {
    // Given
    let (host, address) = a_host_owning_a_queue();

    // When
    let refused = host
        .daemon
        .propagate_work_item(Request::direct(PropagateWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address),
            item_id: "  ".to_string(),
            force: false,
        }))
        .await
        .expect_err("an item is required");

    // Then
    assert_eq!(refused.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn landing_a_backwards_or_zero_based_wave_range_is_refused() {
    // Given
    let (host, address) = a_host_owning_a_queue();

    for (from_wave, to_wave) in [(0, 1), (3, 2)] {
        // When
        let refused = host
            .daemon
            .land_work_queue(Request::direct(LandWorkQueueRequest {
                session_token: TEST_TOKEN.to_string(),
                queue: Some(address.clone()),
                from_wave,
                to_wave,
            }))
            .await
            .expect_err("the range is invalid");

        // Then
        assert_eq!(refused.code(), Code::InvalidArgument, "{from_wave}..{to_wave}");
    }
}
