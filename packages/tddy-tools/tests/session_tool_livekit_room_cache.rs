//! Acceptance tests: reusing one LiveKit connection across a split session's tool calls.
//!
//! `dispatch_via_livekit` connected a room, waited for the remote daemon's participant, issued one
//! `ExecuteTool`, and dropped the room — **per call**. An agent doing fifty `Read`s paid fifty
//! connects. `tddy-tools --mcp` is one long-lived process for the whole session, so it can hold the
//! connection instead; the relay daemon exists precisely because the *other* entry point
//! (`tddy-coder --remote`, many short-lived `claude` invocations) cannot.
//!
//! Reuse has to survive concurrency: an MCP server issues tool calls in parallel, so a cache that
//! only checks "is it connected yet" before connecting would still open several rooms in a burst.
//!
//! These tests inject the connector, so they pin the caching contract without a LiveKit server —
//! what is under test is how often the connector runs and which client comes back, not the wire.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{RpcClientTransport, Status};
use tddy_tools::session_tool_client::{LiveKitRoomCache, LiveKitRoomKey, LiveKitSession};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A transport that answers nothing — these tests are about connection reuse, so the only thing
/// that matters is which instance comes back.
struct InertTransport;

#[async_trait]
impl RpcClientTransport for InertTransport {
    async fn call_unary(&self, _: &str, _: &str, _: Vec<u8>) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented("inert"))
    }
    async fn call_server_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("inert"))
    }
    async fn call_client_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented("inert"))
    }
    async fn call_bidi_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("inert"))
    }
}

/// A cached session whose remote daemon is still in the room.
fn an_inert_session() -> Arc<LiveKitSession> {
    Arc::new(LiveKitSession::new(
        Arc::new(InertTransport) as Arc<dyn RpcClientTransport>,
        Box::new(|| true),
    ))
}

/// A cached session whose remote daemon has since left the room — a daemon restart, mid-session.
fn a_session_whose_daemon_has_left() -> Arc<LiveKitSession> {
    Arc::new(LiveKitSession::new(
        Arc::new(InertTransport) as Arc<dyn RpcClientTransport>,
        Box::new(|| false),
    ))
}

fn a_room_key(server_identity: &str) -> LiveKitRoomKey {
    LiveKitRoomKey {
        url: "ws://livekit.example.invalid".to_string(),
        room: "tddy-lobby".to_string(),
        token: "a-scoped-join-jwt".to_string(),
        server_identity: server_identity.to_string(),
    }
}

/// Counts how many times the cache actually reached for a connection.
#[derive(Default)]
struct ConnectCounter(AtomicUsize);

impl ConnectCounter {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
    fn record(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_second_tool_call_reuses_the_first_connection() {
    // Given a cache that has already connected once
    let cache = LiveKitRoomCache::default();
    let connects = Arc::new(ConnectCounter::default());
    let key = a_room_key("daemon-workstation-b");

    let first = cache
        .client_via(&key, || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Ok(an_inert_session())
            }
        })
        .await
        .expect("the first connect must succeed");

    // When a second tool call asks for a client
    let second = cache
        .client_via(&key, || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Ok(an_inert_session())
            }
        })
        .await
        .expect("the second call must reuse rather than fail");

    // Then — one connection, handed out twice. This is the whole point: an agent doing fifty reads
    // should not pay fifty room connects.
    assert_eq!(
        connects.count(),
        1,
        "the room must be connected exactly once"
    );
    assert!(
        Arc::ptr_eq(&first, &second),
        "both calls must receive the same client instance"
    );
}

#[tokio::test]
async fn concurrent_first_tool_calls_connect_only_once() {
    // Given several tool calls racing before any connection exists — an MCP server issues calls in
    // parallel, so this is the ordinary case rather than an edge one
    let cache = Arc::new(LiveKitRoomCache::default());
    let connects = Arc::new(ConnectCounter::default());
    let key = a_room_key("daemon-workstation-b");

    // When they all ask at once
    let mut calls = Vec::new();
    for _ in 0..8 {
        let cache = Arc::clone(&cache);
        let connects = Arc::clone(&connects);
        let key = key.clone();
        calls.push(tokio::spawn(async move {
            cache
                .client_via(&key, || {
                    let connects = Arc::clone(&connects);
                    async move {
                        connects.record();
                        // Yield so a cache that merely checks-then-connects loses the race here.
                        tokio::task::yield_now().await;
                        Ok(an_inert_session())
                    }
                })
                .await
                .expect("every racing call must get a client")
        }));
    }
    // The tasks are already running concurrently — spawning started them, so collecting them in
    // order does not serialise the race the connector has to survive.
    let mut clients = Vec::new();
    for call in calls {
        clients.push(call.await.expect("no task may panic"));
    }

    // Then — one connection, shared by all eight
    assert_eq!(
        connects.count(),
        1,
        "a burst of parallel tool calls must still connect once"
    );
    for client in &clients[1..] {
        assert!(
            Arc::ptr_eq(&clients[0], client),
            "every racing call must receive the same client instance"
        );
    }
}

#[tokio::test]
async fn a_cached_session_reports_when_its_daemon_has_left_the_room() {
    // Given a connection cached while the codebase daemon was present, which then restarted
    let cache = LiveKitRoomCache::default();
    let key = a_room_key("daemon-workstation-b");
    let session = cache
        .client_via(&key, || async { Ok(a_session_whose_daemon_has_left()) })
        .await
        .expect("connect");

    // Then — the caller can tell before publishing. Reusing one connection means the participant
    // wait now runs once at connect rather than per call, and neither the LiveKit client nor the
    // RPC engine carries a request deadline: publishing to an identity that has gone would hang
    // forever instead of failing, which is strictly worse than the connect-per-call it replaced.
    assert!(
        !session.peer_present(),
        "a cached session must report its remote daemon's absence rather than publish into the void"
    );
}

#[tokio::test]
async fn a_cached_session_reports_its_daemon_as_present_while_it_is_there() {
    // Given the ordinary case
    let cache = LiveKitRoomCache::default();
    let key = a_room_key("daemon-workstation-b");
    let session = cache
        .client_via(&key, || async { Ok(an_inert_session()) })
        .await
        .expect("connect");

    // Then — the check must not reject a healthy session, or every split tool call fails
    assert!(session.peer_present());
}

#[tokio::test]
async fn a_failed_connect_is_not_cached() {
    // Given a first attempt that fails — the daemon was down, or the token had expired
    let cache = LiveKitRoomCache::default();
    let connects = Arc::new(ConnectCounter::default());
    let key = a_room_key("daemon-workstation-b");

    let failed = cache
        .client_via(&key, || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Err("livekit room connect: refused".to_string())
            }
        })
        .await;
    assert!(failed.is_err(), "the first attempt must report the failure");

    // When a later tool call tries again
    let recovered = cache
        .client_via(&key, || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Ok(an_inert_session())
            }
        })
        .await;

    // Then — caching a failure would make one unlucky first call poison the whole session
    assert!(recovered.is_ok(), "a later call must be able to connect");
    assert_eq!(connects.count(), 2, "the failed attempt must not be cached");
}

#[tokio::test]
async fn a_different_remote_daemon_gets_its_own_connection() {
    // Given a cached connection to one codebase host
    let cache = LiveKitRoomCache::default();
    let connects = Arc::new(ConnectCounter::default());

    let first = cache
        .client_via(&a_room_key("daemon-workstation-b"), || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Ok(an_inert_session())
            }
        })
        .await
        .expect("connect");

    // When a call targets a different daemon
    let second = cache
        .client_via(&a_room_key("daemon-laptop-c"), || {
            let connects = Arc::clone(&connects);
            async move {
                connects.record();
                Ok(an_inert_session())
            }
        })
        .await
        .expect("connect");

    // Then — reuse is keyed by destination. Handing back a client aimed at the wrong daemon would
    // execute the tool against the wrong host's filesystem.
    assert_eq!(connects.count(), 2, "a new destination must connect afresh");
    assert!(
        !Arc::ptr_eq(&first, &second),
        "clients for different daemons must not be shared"
    );
}
