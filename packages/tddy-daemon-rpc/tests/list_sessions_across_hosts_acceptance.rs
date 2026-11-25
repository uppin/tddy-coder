//! Acceptance: `ListSessions` with `include_peers` lists every connected daemon's own sessions and
//! names the daemons that did not answer (n6, cross-host session lists).
//!
//! Two daemons, `host-a` and `host-b`, run in this process and reach each other through the
//! in-memory peer forwarder the cross-host suites share. `host-c` is on the roster with no daemon
//! behind it, so it never answers.
//!
//! Feature: docs/ft/daemon/project-concept.md.

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::{
    a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, SESSION_SERVICE, SILENT_HOST,
    THIS_PROJECT,
};
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ListSessionsRequest, ListSessionsResponse, SessionService as _,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    peers: Arc<InProcessPeers>,
    a: Host,
    _b: Host,
    on_a: String,
    on_b: String,
}

/// A session on `host-a` and one on `host-b`.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        peers,
        a,
        _b: b,
        on_a,
        on_b,
    }
}

impl World {
    async fn host_a_lists(&self, include_peers: bool) -> ListSessionsResponse {
        self.a
            .daemon
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: TEST_TOKEN.to_string(),
                include_peers,
            }))
            .await
            .expect("ListSessions is served")
            .into_inner()
    }

    /// `(session, host)` of each of the world's two sessions `listed` holds, sorted.
    fn hosts_of(&self, listed: &ListSessionsResponse) -> Vec<(String, String)> {
        let mut hosts: Vec<(String, String)> = listed
            .sessions
            .iter()
            .filter(|s| s.session_id == self.on_a || s.session_id == self.on_b)
            .map(|s| (s.session_id.clone(), s.daemon_instance_id.clone()))
            .collect();
        hosts.sort();
        hosts
    }

    fn both_sessions_on_their_hosts(&self) -> Vec<(String, String)> {
        let mut expected = vec![
            (self.on_a.clone(), HOST_A.to_string()),
            (self.on_b.clone(), HOST_B.to_string()),
        ];
        expected.sort();
        expected
    }
}

fn unreachable_of(listed: &ListSessionsResponse) -> Vec<&str> {
    listed
        .unreachable_hosts
        .iter()
        .map(|h| h.daemon_instance_id.as_str())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_with_no_daemon_is_named_unreachable_and_the_others_are_still_listed() {
    // Given a session on host-a and one on host-b, and host-c with no daemon behind it
    let world = a_world().await;

    // When host-a lists every connected host's sessions
    let listed = world.host_a_lists(true).await;

    // Then both sessions are listed on their hosts, and host-c is named, with a reason
    assert_eq!(
        world.hosts_of(&listed),
        world.both_sessions_on_their_hosts()
    );
    assert_eq!(unreachable_of(&listed), vec![SILENT_HOST]);
    assert!(
        !listed.unreachable_hosts[0].reason.is_empty(),
        "an unreachable host is named with no reason"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_whose_link_is_down_is_named_and_this_hosts_sessions_are_still_listed() {
    // Given a session on host-a and one on host-b, and the link to host-b down
    let world = a_world().await;
    world.peers.cut_link(HOST_B);

    // When host-a lists every connected host's sessions
    let listed = world.host_a_lists(true).await;

    // Then host-a's session is listed, host-b's is not, and host-b is named beside host-c
    assert_eq!(
        world.hosts_of(&listed),
        vec![(world.on_a.clone(), HOST_A.to_string())]
    );
    let mut unreachable = unreachable_of(&listed);
    unreachable.sort_unstable();
    assert_eq!(unreachable, vec![HOST_B, SILENT_HOST]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_without_peers_asks_no_peer_and_names_none() {
    // Given a session on host-a and one on host-b
    let world = a_world().await;

    // When host-a lists its own sessions only
    let listed = world.host_a_lists(false).await;

    // Then no peer was asked, none is named, and only host-a's session is listed
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, SESSION_SERVICE, "ListSessions"),
        0
    );
    assert!(listed.unreachable_hosts.is_empty());
    assert_eq!(
        world.hosts_of(&listed),
        vec![(world.on_a.clone(), HOST_A.to_string())]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_is_asked_for_its_own_sessions_only_so_a_listing_travels_one_hop() {
    // Given a session on host-a and one on host-b
    let world = a_world().await;

    // When host-a lists every connected host's sessions
    world.host_a_lists(true).await;

    // Then host-b was asked once, and host-b asked no one back
    assert_eq!(
        (
            world
                .peers
                .times_asked(HOST_B, SESSION_SERVICE, "ListSessions"),
            world
                .peers
                .times_asked(HOST_A, SESSION_SERVICE, "ListSessions"),
        ),
        (1, 0)
    );
}
