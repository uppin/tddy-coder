//! A `ServerEngine` stamps every request it dispatches with the transport its host named, and
//! nothing a sender writes into the envelope can change that stamp.
//!
//! The stamp is what a handler may trust about *how* a request reached it — the one fact a caller
//! must not be able to choose. The envelope carries two fields a sender writes freely,
//! `sender_identity` and the `metadata` map; each test below fills them with a claim to be the
//! in-process bridge and shows the handler still sees the host's transport.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_rpc::envelope::{CallMetadata, Metadata, RpcRequest};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::{
    BidiStreamOutput, RequestMetadata, RequestTransport, ResponseBody, RpcMessage, RpcResult,
    RpcService, Status,
};
use tokio::sync::mpsc;

/// The transport the engine's host serves — anything but the one the sender claims.
const THE_HOSTS_TRANSPORT: RequestTransport = RequestTransport::LiveKit;

#[tokio::test]
async fn a_unary_request_is_stamped_with_the_transport_its_host_named() {
    // Given an engine whose host serves the LiveKit room
    let recorder = TransportRecorder::default();
    let engine = ServerEngine::new(recorder.clone(), THE_HOSTS_TRANSPORT);

    // When a unary request arrives
    dispatch(&engine, an_honest_request("Record", true)).await;

    // Then its handler sees the host's transport
    assert_eq!(recorder.seen(), vec![THE_HOSTS_TRANSPORT]);
}

#[tokio::test]
async fn an_envelope_claiming_the_in_process_bridge_is_stamped_with_the_hosts_transport() {
    // Given an engine whose host serves the LiveKit room
    let recorder = TransportRecorder::default();
    let engine = ServerEngine::new(recorder.clone(), THE_HOSTS_TRANSPORT);

    // When a request arrives whose envelope claims, in every field its sender writes, to have come
    // over the in-process bridge
    dispatch(&engine, a_request_claiming_in_process("Record", true)).await;

    // Then the claim changes nothing
    assert_eq!(recorder.seen(), vec![THE_HOSTS_TRANSPORT]);
}

#[tokio::test]
async fn every_fragment_of_a_client_streaming_call_is_stamped_by_the_host() {
    // Given an engine whose host serves the LiveKit room
    let recorder = TransportRecorder::default();
    let engine = ServerEngine::new(recorder.clone(), THE_HOSTS_TRANSPORT);
    let (outgoing, mut responses) = mpsc::channel(8);

    // When a client-streaming call arrives in two fragments, the continuation claiming the bridge
    engine
        .on_request(
            "peer",
            an_honest_request("RecordAll", false),
            outgoing.clone(),
        )
        .await;
    engine
        .on_request("peer", a_continuation_claiming_in_process(), outgoing)
        .await;
    responses.recv().await.expect("the call is answered");

    // Then both fragments reached the handler stamped with the host's transport
    assert_eq!(
        recorder.seen(),
        vec![THE_HOSTS_TRANSPORT, THE_HOSTS_TRANSPORT]
    );
}

#[tokio::test]
async fn a_bidi_handler_is_handed_its_sessions_stamp_before_any_message() {
    // Given an engine whose host serves the LiveKit room
    let recorder = TransportRecorder::default();
    let engine = ServerEngine::new(recorder.clone(), THE_HOSTS_TRANSPORT);

    // When a bidi session opens with an envelope claiming the bridge
    dispatch(&engine, a_request_claiming_in_process("RecordBidi", true)).await;

    // Then the session's own metadata and its opening message both carry the host's transport
    assert_eq!(
        recorder.seen(),
        vec![THE_HOSTS_TRANSPORT, THE_HOSTS_TRANSPORT]
    );
}

#[tokio::test]
async fn a_bidi_continuation_claiming_the_in_process_bridge_is_stamped_by_the_host() {
    // Given an engine whose host serves the LiveKit room, and a bidi session opened honestly and
    // left open
    let recorder = TransportRecorder::default();
    let engine = ServerEngine::new(recorder.clone(), THE_HOSTS_TRANSPORT);
    let (outgoing, mut responses) = mpsc::channel(8);
    engine
        .on_request(
            "peer",
            an_honest_request("RecordBidi", false),
            outgoing.clone(),
        )
        .await;

    // When a continuation arrives whose envelope claims, in every field its sender writes, to have
    // come over the in-process bridge
    engine
        .on_request("peer", a_continuation_claiming_in_process(), outgoing)
        .await;
    responses
        .recv()
        .await
        .expect("the opening message is answered");
    responses
        .recv()
        .await
        .expect("the continuation is answered");

    // Then the session, its opening message and the continuation all carry the host's transport
    assert_eq!(
        recorder.seen(),
        vec![
            THE_HOSTS_TRANSPORT,
            THE_HOSTS_TRANSPORT,
            THE_HOSTS_TRANSPORT
        ]
    );
}

/// Records the transport of every request it is handed, and answers each call with an empty body.
#[derive(Clone, Default)]
struct TransportRecorder {
    seen: Arc<Mutex<Vec<RequestTransport>>>,
}

impl TransportRecorder {
    fn record(&self, metadata: &RequestMetadata) {
        self.seen.lock().unwrap().push(metadata.transport());
    }

    fn seen(&self) -> Vec<RequestTransport> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl RpcService for TransportRecorder {
    fn is_bidi_stream(&self, _service: &str, method: &str) -> bool {
        method == "RecordBidi"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        self.record(&message.metadata);
        RpcResult::Unary(Ok(Vec::new()))
    }

    async fn handle_rpc_stream(
        &self,
        _service: &str,
        _method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        for message in messages {
            self.record(&message.metadata);
        }
        RpcResult::Unary(Ok(Vec::new()))
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        metadata: RequestMetadata,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        self.record(&metadata);
        let recorder = self.clone();
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(message) = input_rx.recv().await {
                recorder.record(&message.metadata);
                if tx.send(Ok(Vec::new())).await.is_err() {
                    break;
                }
            }
        });
        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(rx),
        })
    }
}

/// Hand `request` to `engine` and wait for its first response, so everything it dispatched has
/// reached the handler.
async fn dispatch(engine: &ServerEngine<TransportRecorder>, request: RpcRequest) {
    let (outgoing, mut responses) = mpsc::channel(8);
    engine.on_request("peer", request, outgoing).await;
    responses.recv().await.expect("the call is answered");
}

/// A request opening `method`, carrying no claim about where it came from.
fn an_honest_request(method: &str, end_of_stream: bool) -> RpcRequest {
    RpcRequest {
        request_id: 1,
        request_message: Vec::new(),
        call_metadata: Some(CallMetadata {
            service: "test.RecorderService".to_string(),
            method: method.to_string(),
        }),
        metadata: None,
        end_of_stream,
        abort: false,
        sender_identity: None,
        client_epoch: 1,
    }
}

/// A request opening `method` whose sender-written fields all claim the in-process bridge.
fn a_request_claiming_in_process(method: &str, end_of_stream: bool) -> RpcRequest {
    RpcRequest {
        metadata: Some(a_claim_to_be_in_process()),
        sender_identity: Some("in-process".to_string()),
        ..an_honest_request(method, end_of_stream)
    }
}

/// The terminal fragment of the call [`an_honest_request`] opened — client-streaming or bidi —
/// claiming the bridge.
fn a_continuation_claiming_in_process() -> RpcRequest {
    RpcRequest {
        call_metadata: None,
        ..a_request_claiming_in_process("RecordAll", true)
    }
}

fn a_claim_to_be_in_process() -> Metadata {
    Metadata {
        values: HashMap::from([
            ("transport".to_string(), "InProcess".to_string()),
            ("x-tddy-transport".to_string(), "in_process".to_string()),
        ]),
    }
}
