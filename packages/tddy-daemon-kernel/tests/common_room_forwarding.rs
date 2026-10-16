//! `CommonRoom` forwards to peer daemons through a `PeerForwarder` port, and keeps the forwarding
//! policy itself: the deadline a call gets, the idle timeout a relayed stream gets, and the errors a
//! caller sees. Driven here over an in-memory forwarder — no LiveKit — which is the point of the
//! port. PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-peer-forwarding.md`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_daemon_extensions::{NoPeerForwarding, PeerForwarder};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::{CommonRoom, PEER_FORWARD_STREAM_IDLE_TIMEOUT};
use tddy_rpc::{Code, Status};
use tokio::sync::mpsc;

const PEER: &str = "peer-daemon-1";
const SERVICE: &str = "project.ProjectService";
const METHOD: &str = "AddProjectToHost";

// ---------------------------------------------------------------------------
// An in-memory transport
// ---------------------------------------------------------------------------

/// What a peer answers: bytes now, or never.
enum Answer {
    Bytes(Vec<u8>),
    Never,
    Frames(mpsc::Receiver<Result<Vec<u8>, Status>>),
}

/// A connected transport whose peers answer from a script, and which records what it carried.
#[derive(Default)]
struct AScriptedPeer {
    answers: Mutex<HashMap<(String, String), Answer>>,
    carried: Mutex<Vec<(String, String, String, Vec<u8>)>>,
}

impl AScriptedPeer {
    fn answering(self, service: &str, method: &str, answer: Answer) -> Self {
        self.answers
            .lock()
            .unwrap()
            .insert((service.to_string(), method.to_string()), answer);
        self
    }

    fn carried(&self) -> Vec<(String, String, String, Vec<u8>)> {
        self.carried.lock().unwrap().clone()
    }

    fn take(&self, service: &str, method: &str) -> Answer {
        self.answers
            .lock()
            .unwrap()
            .remove(&(service.to_string(), method.to_string()))
            .expect("the test scripted no answer for this call")
    }
}

#[async_trait::async_trait]
impl PeerForwarder for AScriptedPeer {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.carried.lock().unwrap().push((
            peer_id.to_string(),
            service.to_string(),
            method.to_string(),
            body,
        ));
        match self.take(service, method) {
            Answer::Bytes(bytes) => Ok(bytes),
            Answer::Never => std::future::pending().await,
            Answer::Frames(_) => panic!("a unary call was scripted with a stream"),
        }
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        match self.take(service, method) {
            Answer::Frames(frames) => Ok(frames),
            _ => panic!("a stream was scripted with a unary answer"),
        }
    }
}

fn a_config_with_forward_timeout(secs: u64) -> DaemonConfig {
    DaemonConfig {
        peer_forward_timeout_secs: secs,
        ..Default::default()
    }
}

fn a_room_over(peer: Arc<dyn PeerForwarder>) -> CommonRoom {
    CommonRoom::new(peer, &a_config_with_forward_timeout(30))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_forwarded_call_returns_what_the_peer_answered() {
    // Given a peer that answers AddProjectToHost
    let peer = Arc::new(AScriptedPeer::default().answering(
        SERVICE,
        METHOD,
        Answer::Bytes(b"the peer's answer".to_vec()),
    ));
    let room = a_room_over(peer.clone());

    // When a request is forwarded to it
    let answer = room
        .forward_to_peer(PEER, SERVICE, METHOD, b"the request".to_vec())
        .await
        .expect("the peer answered");

    // Then the caller gets the peer's bytes, and the transport carried exactly that call
    assert_eq!(answer, b"the peer's answer".to_vec());
    assert_eq!(
        peer.carried(),
        vec![(
            PEER.to_string(),
            SERVICE.to_string(),
            METHOD.to_string(),
            b"the request".to_vec()
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn a_peer_that_does_not_answer_within_the_deadline_is_reported_as_timed_out() {
    // Given a peer that never answers
    let room = a_room_over(Arc::new(
        AScriptedPeer::default().answering(SERVICE, METHOD, Answer::Never),
    ));

    // When a call to it is given seven seconds (time is paused, so this does not wait)
    let refused = room
        .forward_to_peer_within(PEER, SERVICE, METHOD, Vec::new(), Duration::from_secs(7))
        .await
        .expect_err("a call nobody answers must time out");

    // Then it says which call, to which daemon, after how long
    assert_eq!(refused.code, Code::DeadlineExceeded);
    assert_eq!(
        refused.message,
        "forwarding project.ProjectService/AddProjectToHost to daemon peer-daemon-1 timed out after 7s: the peer is in the common room but its RPC participant did not answer"
    );
}

#[tokio::test]
async fn forwarding_without_a_connected_transport_is_a_failed_precondition() {
    // Given a common room with no transport wired
    let room = a_room_over(Arc::new(NoPeerForwarding));

    // When a call is forwarded
    let refused = room
        .forward_to_peer(PEER, SERVICE, METHOD, Vec::new())
        .await
        .expect_err("nothing can carry the call");

    // Then the room is reported unreachable, and refused as a precondition
    assert!(!room.is_connected().await);
    assert_eq!(refused.code, Code::FailedPrecondition);
}

#[tokio::test]
async fn a_relayed_stream_delivers_each_frame_then_ends_when_the_peer_ends() {
    // Given a peer streaming two frames and then ending
    let (tx, rx) = mpsc::channel(4);
    tx.send(Ok(b"one".to_vec())).await.unwrap();
    tx.send(Ok(b"two".to_vec())).await.unwrap();
    drop(tx);
    let room = a_room_over(Arc::new(
        AScriptedPeer::default().answering(SERVICE, METHOD, Answer::Frames(rx)),
    ));

    // When the stream is relayed, decoding each frame as text
    let mut relayed = room
        .forward_server_stream_to_peer(PEER, SERVICE, METHOD, Vec::new(), |bytes| {
            Ok(String::from_utf8(bytes).expect("utf-8 frame"))
        })
        .await
        .expect("the stream opened");
    let mut frames = Vec::new();
    while let Some(frame) = relayed.recv().await {
        frames.push(frame.expect("a good frame"));
    }

    // Then both frames arrive in order, and the relay ends with the peer
    assert_eq!(frames, vec!["one".to_string(), "two".to_string()]);
}

#[tokio::test(start_paused = true)]
async fn a_stream_that_goes_quiet_ends_with_one_deadline_error() {
    // Given a peer that opens a stream and then sends nothing
    let (_held_open, rx) = mpsc::channel::<Result<Vec<u8>, Status>>(1);
    let room = a_room_over(Arc::new(
        AScriptedPeer::default().answering(SERVICE, METHOD, Answer::Frames(rx)),
    ));

    // When the relay is read past the idle timeout (time is paused)
    let mut relayed = room
        .forward_server_stream_to_peer(PEER, SERVICE, METHOD, Vec::new(), Ok)
        .await
        .expect("the stream opened");
    let terminal = relayed.recv().await.expect("one terminal frame");
    let after = relayed.recv().await;

    // Then exactly one deadline error ends the relay
    assert_eq!(
        terminal.expect_err("an idle stream ends in an error").message,
        format!(
            "forwarding project.ProjectService/AddProjectToHost to daemon peer-daemon-1 timed out after {}s: the peer is in the common room but its RPC participant did not answer",
            PEER_FORWARD_STREAM_IDLE_TIMEOUT.as_secs()
        )
    );
    assert!(after.is_none(), "nothing follows the terminal error");
}
