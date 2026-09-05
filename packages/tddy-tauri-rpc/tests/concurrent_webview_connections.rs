//! Many concurrent, independently addressed webview connections.
//!
//! `WebviewRpcHost` holds one connection slot, and `connect` abandons whatever was in it. That is
//! why a desktop page can reach the daemon *or* nothing else — there is no way to address a session
//! the way a LiveKit participant inside a room is addressed. These tests pin the host that replaces
//! it: connections keyed by client epoch, each resolving its own target, each released on its own.
//!
//! Reference: `packages/tddy-desktop/docs/webview-ipc-connections.md`

mod support;

use std::sync::Arc;

use support::{
    a_recording_sink, a_request_frame, a_sink_nobody_reads, a_sink_the_page_stopped_reading,
    an_echo_roster, ResponseAssertions,
};
use tddy_rpc::RpcService;
use tddy_tauri_rpc::{
    ConnectError, ConnectionTarget, FrameError, MultiConnectionHost, RosterResolver,
};

/// A resolver that knows which sessions are live, and refuses every other one.
///
/// Deliberately **not** named after the desktop's own resolver (`DaemonRosters`, in
/// `packages/tddy-desktop/src-tauri/src/ipc.rs`), because it is not a stand-in for it: that one
/// resolves *every* target, since the only lookup that could say whether a session is live is async
/// and `roster_for` is not. So `NoSuchTarget` is a property of this fixture — of what the crate does
/// with a `None` it is handed — and not a guarantee the shipping resolver has.
struct LiveSessionRosters {
    live_sessions: Vec<String>,
}

impl RosterResolver for LiveSessionRosters {
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

fn a_host_serving(sessions: &[&str]) -> MultiConnectionHost<LiveSessionRosters> {
    MultiConnectionHost::new(LiveSessionRosters {
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
    // Given a page with two sessions to reach — several open terminals in the drawer
    let host = a_host_serving(&["session-0001", "session-0002"]);

    // When it attaches both, each on its own epoch
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 1)
        .await
        .expect("the first session resolves");
    host.connect(a_session("session-0002"), a_sink_nobody_reads(), 2)
        .await
        .expect("the second session resolves");

    // Then both are live, addressed independently
    assert_eq!(host.connection_count().await, 2);
}

#[tokio::test]
async fn refuses_a_target_no_roster_serves_at_connect_time() {
    // Given a host serving one live session
    let host = a_host_serving(&["session-0001"]);

    // When a page asks to reach a session that has ended
    let refused = host
        .connect(a_session("session-that-ended"), a_sink_nobody_reads(), 1)
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
    // Given a page already holding a connection on epoch 7
    let host = a_host_serving(&["session-0001"]);
    host.connect(ConnectionTarget::Daemon, a_sink_nobody_reads(), 7)
        .await
        .expect("the first connection is accepted");

    // When it opens another reusing that epoch — epochs are minted per transport, so this is a bug
    // on the page side, and accepting it would route two connections' answers to one sink
    let refused = host
        .connect(a_session("session-0001"), a_sink_nobody_reads(), 7)
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
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 2)
        .await
        .expect("first session");
    host.connect(a_session("session-0002"), a_sink_nobody_reads(), 3)
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
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 1)
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
    // Given a page holding a daemon connection and two sessions
    let host = a_host_serving(&["session-0001", "session-0002"]);
    host.connect(ConnectionTarget::Daemon, a_sink_nobody_reads(), 1)
        .await
        .expect("daemon");
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 2)
        .await
        .expect("first session");
    host.connect(a_session("session-0002"), a_sink_nobody_reads(), 3)
        .await
        .expect("second session");

    // When the page reloads
    host.disconnect_all().await;

    // Then nothing survives. The single-slot host reaped the previous page implicitly, because
    // `connect` overwrote the one slot; with a map it has to be explicit, and a leaked per-session
    // connection on every reload is what this prevents.
    assert_eq!(host.connection_count().await, 0);
}

#[tokio::test]
async fn names_the_only_connection_it_has_when_refusing_a_frame_for_an_unknown_epoch() {
    // Given a host serving exactly one connection
    let host = a_host_serving(&["session-0001"]);
    host.connect(ConnectionTarget::Daemon, a_sink_nobody_reads(), 1)
        .await
        .expect("daemon");

    // When a frame arrives stamped with an epoch nothing here answers for
    let refused = host
        .handle_request_frame(
            &a_request_frame()
                .with_epoch(99)
                .with_id(12)
                .with_payload(b"orphan")
                .build(),
        )
        .await;

    // Then it is refused rather than dispatched onto the connection that does exist — whose sink
    // would drop it for an epoch mismatch, leaving the caller waiting for an answer that cannot
    // arrive — and the refusal names that connection, which with one open is the truth
    assert_eq!(
        refused,
        Err(FrameError::StaleConnection {
            connected: 1,
            frame: 99
        })
    );
}

#[tokio::test]
async fn names_no_connection_when_refusing_a_frame_for_an_unknown_epoch_while_several_are_open() {
    // Given a page holding a daemon connection and a session at once
    let host = a_host_serving(&["session-0001"]);
    host.connect(ConnectionTarget::Daemon, a_sink_nobody_reads(), 1)
        .await
        .expect("daemon");
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 2)
        .await
        .expect("session");

    // When a frame arrives stamped with an epoch neither of them answers for
    let refused = host
        .handle_request_frame(
            &a_request_frame()
                .with_epoch(99)
                .with_id(12)
                .with_payload(b"orphan")
                .build(),
        )
        .await;

    // Then it is still refused, but without naming a connection: `StaleConnection` names *the*
    // connected epoch, and a host serving several has no single one to name — reporting either
    // would tell the page something untrue about what it is connected to
    assert_eq!(refused, Err(FrameError::NotConnected));
}

#[tokio::test]
async fn keeps_a_call_in_flight_delivering_while_the_page_opens_another_connection() {
    // Given a daemon connection with a bidirectional call open — a call that stays in flight
    // between its messages, which is what makes "in flight" observable at all
    let host = a_host_serving(&["session-0001"]);
    let (daemon_sink, mut daemon_frames) = a_recording_sink();
    host.connect(ConnectionTarget::Daemon, daemon_sink, 1)
        .await
        .expect("the daemon roster always resolves");
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(1)
            .with_id(20)
            .calling("EchoEach")
            .with_payload(b"before")
            .opening_a_request_stream()
            .build(),
    )
    .await
    .expect("the opening frame was not accepted");
    daemon_frames
        .next_response()
        .await
        .assert_epoch(1)
        .assert_message(b"BEFORE");

    // When the page attaches a session while that call is still open
    host.connect(a_session("session-0001"), a_sink_nobody_reads(), 2)
        .await
        .expect("a live session resolves");

    // Then the open call goes on delivering onto the connection it was made on. Opening a
    // connection disturbs no other — the single-slot host, where `connect` abandons, would have
    // dropped this call's answers on the floor.
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(1)
            .with_id(20)
            .with_payload(b"after")
            .closing_a_request_stream()
            .build(),
    )
    .await
    .expect("the closing frame was not accepted");
    assert_eq!(
        daemon_frames.stream_payloads_until_closed().await,
        vec![b"AFTER".to_vec()]
    );
}

/// More answers than one connection's response queue can hold — `RESPONSE_QUEUE_CAPACITY` in
/// `src/multi_host.rs` is 256, and cannot be imported here because it is the host's own business.
/// One queue shared by every connection would be full long before this many, and every other
/// connection would then be publishing into a queue with no room left in it.
const MORE_ANSWERS_THAN_ONE_QUEUE_HOLDS: i32 = 300;

/// Issue that many calls on `client_epoch` — answers the page it belongs to will never come for.
async fn pile_up_unread_answers_on(
    host: &MultiConnectionHost<LiveSessionRosters>,
    client_epoch: u32,
) {
    for request_id in 0..MORE_ANSWERS_THAN_ONE_QUEUE_HOLDS {
        host.handle_request_frame(
            &a_request_frame()
                .with_epoch(client_epoch)
                .with_id(request_id)
                .build(),
        )
        .await
        .expect("a connection whose page stopped reading still accepts frames");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_channel_the_page_stopped_reading_does_not_stall_the_others() {
    // Given a page holding two connections, one of whose channels it has stopped reading
    let host = a_host_serving(&["session-0001"]);
    let (stalled_sink, mut stalled_page) = a_sink_the_page_stopped_reading();
    host.connect(a_session("session-0001"), stalled_sink, 1)
        .await
        .expect("a live session resolves");
    let (daemon_sink, mut daemon_frames) = a_recording_sink();
    host.connect(ConnectionTarget::Daemon, daemon_sink, 2)
        .await
        .expect("the daemon roster always resolves");

    // When answers pile up unread on that channel, past what one connection's queue can hold, and
    // the connection the page is still reading makes a call
    pile_up_unread_answers_on(&host, 1).await;
    stalled_page.once_a_frame_is_stuck_in_the_channel().await;
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(2)
            .with_id(1)
            .with_payload(b"unstalled")
            .build(),
    )
    .await
    .expect("the connection the page is reading accepts its frame");

    // Then that call is answered anyway, and both connections are still open. The bounded queue is
    // per connection: one shared queue would be full of answers the stalled page will never take,
    // and this call would wait behind them for as long as that page held its channel open.
    daemon_frames
        .next_response()
        .await
        .assert_epoch(2)
        .assert_message(b"unstalled")
        .assert_complete();
    assert_eq!(host.connection_count().await, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_departing_connection_does_not_reap_the_one_that_took_its_epoch() {
    // Given a connection whose page stopped taking frames with an answer already on its way out
    let host = a_host_serving(&["session-0001", "session-0002"]);
    let (stalled_sink, mut departing_page) = a_sink_the_page_stopped_reading();
    host.connect(a_session("session-0001"), stalled_sink, 1)
        .await
        .expect("a live session resolves");
    host.handle_request_frame(&a_request_frame().with_epoch(1).with_id(1).build())
        .await
        .expect("the connection accepts its frame");
    departing_page.once_a_frame_is_stuck_in_the_channel().await;

    // When the page detaches that session and attaches another on the same epoch — a released epoch
    // is free to be minted again — and only then does the first connection's departure surface
    host.disconnect(1).await;
    let (successor_sink, mut successor_frames) = a_recording_sink();
    host.connect(a_session("session-0002"), successor_sink, 1)
        .await
        .expect("the released epoch is free again");
    departing_page
        .once_the_host_has_handled_the_departure()
        .await;

    // Then the successor is untouched and still serves. A departing connection removes *itself*,
    // not whatever the map happens to hold under its epoch by then: an epoch is a key, not a
    // generation, so without that distinction a page would lose the connection it just opened.
    assert_eq!(host.connection_count().await, 1);
    host.handle_request_frame(
        &a_request_frame()
            .with_epoch(1)
            .with_id(2)
            .with_payload(b"successor")
            .build(),
    )
    .await
    .expect("the successor connection still accepts frames");
    successor_frames
        .next_response()
        .await
        .assert_epoch(1)
        .assert_message(b"successor")
        .assert_complete();
}
