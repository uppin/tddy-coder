//! A supervisor that stops answering fails the daemon's call within a deadline, naming the call —
//! it never leaves a session start waiting on it forever.
//!
//! The peer here is not a supervisor: it is a unix socket that accepts the connection and then says
//! nothing (or, for the clone stream, says one thing and then nothing). No process is forked, so the
//! file runs on every platform.
//!
//! Every test is itself bounded by [`A_CALL_THAT_HANGS`]: a missing deadline fails the test, it does
//! not hang it.
//!
//! Feature docs: docs/ft/supervisor/tddy-supervisor.md

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcMessage, RpcResult, RpcService};
use tddy_stdio::StdioEndpoint;
use tddy_supervisor::proto::supervisor::clone_event::Event;
use tddy_supervisor::proto::supervisor::{CloneEvent, CloneRepositoryRequest, CloneStarted};
use tddy_supervisor::{SpawnRelayedRequest, SupervisorClient, SupervisorError};
use tokio::net::UnixListener;

/// The deadline the client under test is given.
const A_SHORT_DEADLINE: Duration = Duration::from_millis(300);

/// Past this the call is not bounded at all.
const A_CALL_THAT_HANGS: Duration = Duration::from_secs(10);

/// A unix socket whose every connection is accepted and then held open in silence.
struct SilentPeer {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    _accepting: tokio::task::JoinHandle<()>,
}

fn a_supervisor_that_never_answers() -> SilentPeer {
    let dir = tempfile::tempdir().expect("a temp dir");
    let socket = dir.path().join("supervisor.sock");
    let listener = UnixListener::bind(&socket).expect("the silent peer binds");
    let accepting = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    SilentPeer {
        _dir: dir,
        socket,
        _accepting: accepting,
    }
}

/// A supervisor whose `CloneRepository` reports that git started, then goes quiet with the stream
/// still open.
struct QuietAfterStartPeer {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    _accepting: tokio::task::JoinHandle<()>,
}

fn a_supervisor_whose_clone_goes_quiet_after_starting() -> QuietAfterStartPeer {
    let dir = tempfile::tempdir().expect("a temp dir");
    let socket = dir.path().join("supervisor.sock");
    let listener = UnixListener::bind(&socket).expect("the peer binds");
    let accepting = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (reader, writer) = stream.into_split();
            let (_client, endpoint) = StdioEndpoint::from_duplex(
                reader,
                writer,
                StartsThenGoesQuiet,
                tddy_rpc::RequestTransport::UnixSocket,
            );
            tokio::spawn(endpoint.run());
        }
    });
    QuietAfterStartPeer {
        _dir: dir,
        socket,
        _accepting: accepting,
    }
}

struct StartsThenGoesQuiet;

#[async_trait]
impl RpcService for StartsThenGoesQuiet {
    async fn handle_rpc(&self, _service: &str, _method: &str, _message: &RpcMessage) -> RpcResult {
        let (frames, receiver) = tokio::sync::mpsc::channel(4);
        let started = CloneEvent {
            event: Some(Event::Started(CloneStarted { pid: 4242 })),
        };
        frames
            .send(Ok(started.encode_to_vec()))
            .await
            .expect("the first frame is queued");
        // The sender is kept, so the stream stays open with nothing more on it.
        tokio::spawn(async move {
            let _open = frames;
            std::future::pending::<()>().await;
        });
        RpcResult::ServerStream(Ok(receiver))
    }
}

async fn a_client_of(socket: &Path) -> SupervisorClient {
    SupervisorClient::connect(socket)
        .await
        .expect("the socket accepts the connection")
        .with_call_deadline(A_SHORT_DEADLINE)
        .with_clone_progress_deadline(A_SHORT_DEADLINE)
}

fn a_relayed_spawn() -> SpawnRelayedRequest {
    SpawnRelayedRequest {
        os_user: "ada".to_string(),
        tool_path: PathBuf::from("/usr/bin/a-relayed-tool"),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        relay_socket: PathBuf::from("/tmp/relay.sock"),
        scope: None,
    }
}

fn a_clone() -> CloneRepositoryRequest {
    CloneRepositoryRequest {
        os_user: "ada".to_string(),
        git_url: "file:///nowhere".to_string(),
        destination: "/tmp/nowhere".to_string(),
        ..Default::default()
    }
}

fn unavailable_message(error: SupervisorError) -> String {
    match error {
        SupervisorError::Unavailable { message } => message,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_supervisor_that_never_answers_fails_spawn_relayed_naming_the_method() {
    // Given a supervisor that accepts the connection and never answers
    let peer = a_supervisor_that_never_answers();
    let client = a_client_of(&peer.socket).await;

    // When a relayed spawn is asked of it
    let answered = tokio::time::timeout(A_CALL_THAT_HANGS, client.spawn_relayed(a_relayed_spawn()))
        .await
        .expect(
            "SpawnRelayed waited on a supervisor that never answers with no deadline of its own",
        );

    // Then the call failed Unavailable, naming the method and how long it waited
    let message = unavailable_message(answered.expect_err("no answer is not a success"));
    assert!(message.contains("SpawnRelayed"), "{message}");
    assert!(message.contains("300ms"), "{message}");
}

#[tokio::test]
async fn every_unary_call_is_bounded_not_only_spawn_relayed() {
    // Given a supervisor that never answers
    let peer = a_supervisor_that_never_answers();
    let client = a_client_of(&peer.socket).await;

    // When the cheapest call is made of it
    let answered = tokio::time::timeout(A_CALL_THAT_HANGS, client.list_services())
        .await
        .expect("ListServices hung with no deadline of its own");

    // Then it failed naming that call
    let message = unavailable_message(answered.expect_err("no answer is not a success"));
    assert!(message.contains("ListServices"), "{message}");
}

#[tokio::test]
async fn a_supervisor_clone_with_no_first_frame_fails_within_its_deadline() {
    // Given a supervisor that accepts the connection and never answers
    let peer = a_supervisor_that_never_answers();
    let client = a_client_of(&peer.socket).await;

    // When a clone is asked of it
    let answered = tokio::time::timeout(A_CALL_THAT_HANGS, client.clone_repository(a_clone()))
        .await
        .expect(
            "CloneRepository waited for a first event that never comes with no deadline of its own",
        );

    // Then the clone failed Unavailable, naming the method
    let message = unavailable_message(answered.expect_err("no first event is not a started clone"));
    assert!(message.contains("CloneRepository"), "{message}");
}

#[tokio::test]
async fn a_supervisor_clone_that_goes_quiet_after_starting_ends_its_stream_with_unavailable() {
    // Given a clone the supervisor started and then stopped reporting on, its stream left open
    let peer = a_supervisor_whose_clone_goes_quiet_after_starting();
    let client = a_client_of(&peer.socket).await;
    let mut events = tokio::time::timeout(A_CALL_THAT_HANGS, client.clone_repository(a_clone()))
        .await
        .expect("the clone opens")
        .expect("the clone starts");
    let first = events
        .recv()
        .await
        .expect("the stream is open")
        .expect("a clone event");
    assert!(matches!(first.event, Some(Event::Started(_))), "{first:?}");

    // When the caller waits for its next event
    let next = tokio::time::timeout(A_CALL_THAT_HANGS, events.recv())
        .await
        .expect("the clone stream stayed silent with no deadline between events");

    // Then the stream ended its silence with Unavailable naming the clone
    let message = unavailable_message(
        next.expect("an item, not a closed stream")
            .expect_err("silence is not an event"),
    );
    assert!(message.contains("CloneRepository"), "{message}");
}

#[test]
fn a_client_connects_with_the_production_deadlines_until_told_otherwise() {
    // Guard: the defaults are the published constants, so a daemon that never calls a setter is
    // bounded too.
    assert_eq!(
        tddy_supervisor::client::SUPERVISOR_CALL_DEADLINE,
        Duration::from_secs(60)
    );
    assert_eq!(
        tddy_supervisor::client::SUPERVISOR_CLONE_PROGRESS_DEADLINE,
        Duration::from_secs(300)
    );
}
