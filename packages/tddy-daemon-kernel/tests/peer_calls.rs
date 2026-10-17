//! Typed RPCs forwarded to a peer daemon: the request is encoded, carried to that peer's service and
//! method, and the answer decoded — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-forwarding-consumers.md`. In memory: the
//! common room forwards through a scripted `PeerForwarder`.

use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_calls::{forward_add_project_to_host, forward_stream_read_host_document};
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_rpc::{Code, Status};
use tddy_service::proto::project::{AddProjectToHostRequest, AddProjectToHostResponse};
use tddy_service::proto::session_files::{HostDocumentChunk, ReadHostDocumentRequest};
use tokio::sync::mpsc;

const PEER: &str = "peer-daemon-1";

/// Answers the one call it is scripted for and records what it carried.
struct OnePeerCall {
    answer: Mutex<Option<Result<Vec<u8>, Vec<Vec<u8>>>>>,
    carried: Mutex<Vec<(String, String, String, Vec<u8>)>>,
}

impl OnePeerCall {
    fn answering_unary(bytes: Vec<u8>) -> Self {
        Self { answer: Mutex::new(Some(Ok(bytes))), carried: Mutex::new(Vec::new()) }
    }

    fn answering_frames(frames: Vec<Vec<u8>>) -> Self {
        Self { answer: Mutex::new(Some(Err(frames))), carried: Mutex::new(Vec::new()) }
    }

    fn carried(&self) -> Vec<(String, String, String, Vec<u8>)> {
        self.carried.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl PeerForwarder for OnePeerCall {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(&self, peer: &str, service: &str, method: &str, body: Vec<u8>) -> Result<Vec<u8>, Status> {
        self.carried.lock().unwrap().push((peer.into(), service.into(), method.into(), body));
        Ok(self.answer.lock().unwrap().take().unwrap().expect("scripted for a unary call"))
    }

    async fn call_server_stream(
        &self,
        peer: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.carried.lock().unwrap().push((peer.into(), service.into(), method.into(), body));
        let frames = self.answer.lock().unwrap().take().unwrap().expect_err("scripted for a stream");
        let (tx, rx) = mpsc::channel(frames.len().max(1));
        for frame in frames {
            tx.send(Ok(frame)).await.unwrap();
        }
        Ok(rx)
    }
}

fn a_room_over(peer: Arc<OnePeerCall>) -> CommonRoom {
    CommonRoom::new(peer, &DaemonConfig::default())
}

#[tokio::test]
async fn adding_a_project_on_a_peer_sends_the_request_to_that_peer_and_decodes_its_answer() {
    // Given a peer that answers AddProjectToHost
    let answer = AddProjectToHostResponse::default();
    let peer = Arc::new(OnePeerCall::answering_unary(answer.encode_to_vec()));
    let request = AddProjectToHostRequest {
        project_id: "my-app".to_string(),
        ..Default::default()
    };

    // When the request is forwarded
    let decoded = forward_add_project_to_host(&a_room_over(peer.clone()), PEER, &request)
        .await
        .expect("the peer answered");

    // Then that peer's ProjectService/AddProjectToHost got exactly the encoded request
    assert_eq!(decoded, answer);
    assert_eq!(
        peer.carried(),
        vec![(
            PEER.to_string(),
            "project.ProjectService".to_string(),
            "AddProjectToHost".to_string(),
            request.encode_to_vec()
        )]
    );
}

#[tokio::test]
async fn a_peer_answering_garbage_is_reported_as_an_internal_decode_error() {
    // Given a peer that answers with bytes that are not an AddProjectToHostResponse
    let peer = Arc::new(OnePeerCall::answering_unary(vec![0xff, 0xff, 0xff]));

    // When a request is forwarded
    let refused = forward_add_project_to_host(
        &a_room_over(peer),
        PEER,
        &AddProjectToHostRequest::default(),
    )
    .await
    .expect_err("garbage does not decode");

    // Then it is an internal error naming the response type. `starts_with`: the rest is prost's
    // own decode message, which is not this crate's to pin.
    assert_eq!(refused.code, Code::Internal);
    assert!(
        refused.message.starts_with("decode AddProjectToHostResponse:"),
        "got: {}",
        refused.message
    );
}

#[tokio::test]
async fn reading_a_host_document_from_a_peer_relays_every_frame_decoded() {
    // Given a peer streaming two document chunks
    let first = HostDocumentChunk { data: b"one".to_vec(), ..Default::default() };
    let second = HostDocumentChunk { data: b"two".to_vec(), ..Default::default() };
    let peer = Arc::new(OnePeerCall::answering_frames(vec![
        first.encode_to_vec(),
        second.encode_to_vec(),
    ]));

    // When the document is read through it
    let mut frames = forward_stream_read_host_document(
        &a_room_over(peer),
        PEER,
        &ReadHostDocumentRequest::default(),
    )
    .await
    .expect("the stream opened");
    let mut chunks = Vec::new();
    while let Some(chunk) = frames.recv().await {
        chunks.push(chunk.expect("a good chunk"));
    }

    // Then both chunks arrive, decoded, in order
    assert_eq!(chunks, vec![first, second]);
}
