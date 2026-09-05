//! Many concurrent, independently addressed webview connections.
//!
//! `WebviewRpcHost` holds one connection slot, and `connect` abandons whatever was in it. That is
//! why a desktop page can reach the daemon *or* nothing else — there is no way to address a session
//! the way a LiveKit participant inside a room is addressed. These tests pin the host that replaces
//! it: connections keyed by client epoch, each resolving its own target, each released on its own.
//!
//! Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-multi-connection-ipc.md`

mod support;

use std::sync::Arc;

use support::{a_recording_sink, a_request_frame, an_echo_roster, ResponseAssertions};
use tddy_rpc::RpcService;
use tddy_tauri_rpc::{ConnectError, ConnectionTarget, MultiConnectionHost, RosterResolver};

/// The daemon's own view: it serves its roster, and knows which sessions exist.
struct DaemonRosters {
    live_sessions: Vec<String>,
}

impl RosterResolver for DaemonRosters {
    fn roster_for(&self, target: &ConnectionTarget) -> Option<Arc<dyn RpcService>> {
        match target {
            ConnectionTarget::Daemon => Some(an_echo_roster()),
            ConnectionTarget::Session { session_id } => self
                .live_sessions
                .iter()
                .any(|id| id == session_id)
                .then(an_echo_roster),
        }
    }
}

fn a_host_serving(sessions: &[&str]) -> MultiConnectionHost<DaemonRosters> {
    MultiConnectionHost::new(DaemonRosters {
        live_sessions: sessions.iter().map(|s| s.to_string()).collect(),
    })
}

fn a_session(id: &str) -> ConnectionTarget {
    ConnectionTarget::Session {
        session_id: id.to_string(),
    }
}

#[tokio::test]
async fn serves_the_daemon_and_a_session_at_the_same_time() {
    // Given a page that has opened its daemon connection and then attached a session
    let host = a_host_serving(&["session-0001"]);
    let (daemon_sink, mut daemon_frames) = a_recording_sink();
    let (session_sink, mut session_frames) = a_recording_sink();

    host.connect(ConnectionTarget::Daemon, daemon_sink.clone(), 1)
        .await
        .expect("the daemon roster always resolves");
    host.connect(a_session("session-0001"), session_sink.clone(), 2)
        .await
        .expect("a live session resolves");

    // When each connection issues a call
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(1)
            .with_id(10)
            .with_payload(b"to the daemon")
            .build(),
    )
    .await
    .expect("the daemon connection accepts its frame");
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(2)
            .with_id(10)
            .with_payload(b"to the session")
            .build(),
    )
    .await
    .expect("the session connection accepts its frame");

    // Then each answer goes to its own sink. Opening the second connection did not disturb the
    // first — the behaviour the single-slot host cannot have, since `connect` abandons.
    daemon_frames
        .next_response()
        .await
        .assert_epoch(1)
        .assert_message(b"to the daemon");
    session_frames
        .next_response()
        .await
        .assert_epoch(2)
        .assert_message(b"to the session");
    assert_eq!(host.connection_count().await, 2);
}

#[tokio::test]
async fn holds_one_connection_per_attached_session() {
    // Given two sessions attached at once — several open terminals in the drawer
    let host = a_host_serving(&["session-0001", "session-0002"]);

    host.connect(a_session("session-0001"), a_recording_sink().0, 1)
        .await
        .expect("the first session resolves");
    host.connect(a_session("session-0002"), a_recording_sink().0, 2)
        .await
        .expect("the second session resolves");

    // Then both are live, addressed independently
    assert_eq!(host.connection_count().await, 2);
}

#[tokio::test]
async fn refuses_a_target_no_roster_serves_at_connect_time() {
    // Given a page asking for a session that has ended
    let host = a_host_serving(&["session-0001"]);

    let refused = host
        .connect(a_session("session-that-ended"), a_recording_sink().0, 1)
        .await;

    // Then the connection is refused with a reason, rather than accepted and silently answering
    // nothing. A caller that is told can fail; a caller that is not waits forever.
    assert_eq!(
        refused,
        Err(ConnectError::NoSuchTarget {
            target: a_session("session-that-ended")
        })
    );
    assert_eq!(host.connection_count().await, 0);
}

#[tokio::test]
async fn refuses_an_epoch_that_is_already_connected() {
    // Given a page that reuses a client epoch — epochs are minted per transport, so this is a bug
    // on the page side, and accepting it would route two connections' answers to one sink
    let host = a_host_serving(&["session-0001"]);
    host.connect(ConnectionTarget::Daemon, a_recording_sink().0, 7)
        .await
        .expect("the first connection is accepted");

    let refused = host
        .connect(a_session("session-0001"), a_recording_sink().0, 7)
        .await;

    assert_eq!(refused, Err(ConnectError::EpochInUse { client_epoch: 7 }));
}

#[tokio::test]
async fn releasing_one_connection_leaves_the_others_serving() {
    // Given a daemon connection and two session connections
    let host = a_host_serving(&["session-0001", "session-0002"]);
    let (daemon_sink, mut daemon_frames) = a_recording_sink();
    host.connect(ConnectionTarget::Daemon, daemon_sink.clone(), 1)
        .await
        .expect("daemon");
    host.connect(a_session("session-0001"), a_recording_sink().0, 2)
        .await
        .expect("first session");
    host.connect(a_session("session-0002"), a_recording_sink().0, 3)
        .await
        .expect("second session");

    // When one session is detached
    host.disconnect(2).await;

    // Then only that one is gone, and the daemon connection still answers. Without an explicit
    // release every attach would leak a host-side peer — a leak the single-slot host never had,
    // because there was only ever one connection to leak.
    assert_eq!(host.connection_count().await, 2);
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(1)
            .with_id(11)
            .with_payload(b"still here")
            .build(),
    )
    .await
    .expect("the daemon connection still accepts frames");
    daemon_frames
        .next_response()
        .await
        .assert_epoch(1)
        .assert_message(b"still here");
}

#[tokio::test]
async fn releasing_a_connection_twice_is_harmless() {
    // Given a detached session
    let host = a_host_serving(&["session-0001"]);
    host.connect(a_session("session-0001"), a_recording_sink().0, 1)
        .await
        .expect("session");
    host.disconnect(1).await;

    // When the page releases it again — a detach racing an unmount
    host.disconnect(1).await;

    // Then nothing happens twice and nothing panics
    assert_eq!(host.connection_count().await, 0);
}

#[tokio::test]
async fn a_page_reload_reaps_every_connection_the_previous_page_owned() {
    // Given a page holding a daemon connection and two sessions, which then reloads
    let host = a_host_serving(&["session-0001", "session-0002"]);
    host.connect(ConnectionTarget::Daemon, a_recording_sink().0, 1)
        .await
        .expect("daemon");
    host.connect(a_session("session-0001"), a_recording_sink().0, 2)
        .await
        .expect("first session");
    host.connect(a_session("session-0002"), a_recording_sink().0, 3)
        .await
        .expect("second session");

    host.disconnect_all().await;

    // Then nothing survives. The single-slot host reaped the previous page implicitly, because
    // `connect` overwrote the one slot; with a map it has to be explicit, and a leaked per-session
    // connection on every reload is what this prevents.
    assert_eq!(host.connection_count().await, 0);
}

#[tokio::test]
async fn refuses_a_frame_stamped_with_an_epoch_it_does_not_have() {
    // Given one live connection
    let host = a_host_serving(&["session-0001"]);
    host.connect(ConnectionTarget::Daemon, a_recording_sink().0, 1)
        .await
        .expect("daemon");

    // When a frame arrives for a connection that was released — a call issued as a session detached
    let refused = host
        .handle_request_frame(
            &a_request_frame()
                .with_epoch(99)
                .with_id(12)
                .with_payload(b"orphan")
                .build(),
        )
        .await;

    // Then it is refused rather than dispatched onto some other connection, whose sink would drop
    // it for an epoch mismatch and leave the caller waiting for an answer that cannot arrive
    assert!(
        refused.is_err(),
        "a frame for an unknown connection must be refused"
    );
}
