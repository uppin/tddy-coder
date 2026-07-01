//! Transport-agnostic dispatch for incoming RPC requests. Routes decoded `RpcRequest`s into an
//! [`RpcBridge<S>`] and multiplexes concurrent unary/stream/bidi state by `(peer, request_id)` —
//! a peer identifier is required because request ids are only unique per-peer, not globally.
//! Results are published by sending `(peer, response)` pairs into a caller-supplied channel; this
//! engine never touches a transport directly.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::bridge::{BidiStreamOutput, ResponseBody, RpcBridge, RpcService};
use crate::envelope::{RpcError, RpcRequest, RpcResponse};
use crate::message::{RequestMetadata, RpcMessage};
use crate::status::Status;

/// Composite key for multiplexing: request ids are only unique per-peer.
type SessionKey = (String, i32);

/// Live bidi session: the input channel to an already-running handler.
struct BidiSession {
    input_tx: mpsc::Sender<RpcMessage>,
}

/// Messages collected so far for an in-progress non-bidi multi-message (client-streaming) call.
/// Unlike a bidi session, there's no running handler to feed incrementally — `RpcService`'s
/// non-bidi contract (`handle_rpc_stream`) takes the whole message slice at once, so fragments
/// are accumulated here until the terminal one arrives, then dispatched together in one call.
struct PendingMultiMessage {
    messages: Vec<RpcMessage>,
    service: String,
    method: String,
}

fn to_rpc_message(request: &RpcRequest) -> RpcMessage {
    RpcMessage {
        payload: request.request_message.clone(),
        metadata: RequestMetadata {
            sender_identity: request.sender_identity.clone(),
        },
    }
}

/// Routes decoded `RpcRequest`s into an [`RpcBridge<S>`], transport-agnostically.
pub struct ServerEngine<S: RpcService> {
    bridge: Arc<RpcBridge<S>>,
    active_bidi_sessions: Mutex<HashMap<SessionKey, BidiSession>>,
    pending_multi_message: Mutex<HashMap<SessionKey, PendingMultiMessage>>,
}

impl<S: RpcService> ServerEngine<S> {
    pub fn new(service: S) -> Self {
        Self {
            bridge: Arc::new(RpcBridge::new(service)),
            active_bidi_sessions: Mutex::new(HashMap::new()),
            pending_multi_message: Mutex::new(HashMap::new()),
        }
    }

    /// Handle one decoded incoming request from `peer`, publishing every resulting response
    /// (immediate or streamed over time) onto `outgoing`.
    pub async fn on_request(
        &self,
        peer: &str,
        request: RpcRequest,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        let request_id = request.request_id;
        let session_key = (peer.to_string(), request_id);

        if self.route_bidi_continuation(&session_key, &request).await {
            return;
        }
        if self
            .route_multi_message_continuation(&session_key, &request, &outgoing)
            .await
        {
            return;
        }

        let service = request
            .call_metadata
            .as_ref()
            .map(|m| m.service.as_str())
            .unwrap_or("");
        let method = request
            .call_metadata
            .as_ref()
            .map(|m| m.method.as_str())
            .unwrap_or("");
        let opens_bidi_session =
            request.call_metadata.is_some() && self.bridge.is_bidi_stream(service, method);

        if opens_bidi_session {
            self.open_bidi_session(peer, request, outgoing).await;
            return;
        }

        let message = to_rpc_message(&request);

        if !request.end_of_stream {
            // First fragment of a non-bidi multi-message (client-streaming) call: start
            // accumulating. `RpcService::handle_rpc_stream` needs every message at once, so
            // dispatch is deferred until the terminal fragment arrives above.
            self.pending_multi_message.lock().await.insert(
                session_key,
                PendingMultiMessage {
                    messages: vec![message],
                    service: service.to_string(),
                    method: method.to_string(),
                },
            );
            return;
        }

        // Single-message dispatch: unary or non-bidi server-streaming, already complete in one
        // frame.
        self.spawn_dispatch(
            peer.to_string(),
            request_id,
            service.to_string(),
            method.to_string(),
            vec![message],
            outgoing,
        );
    }

    /// Routes a continuation of an already-open bidi session (no `call_metadata`) directly into
    /// it. Returns `true` if handled — the caller should stop processing this request.
    async fn route_bidi_continuation(
        &self,
        session_key: &SessionKey,
        request: &RpcRequest,
    ) -> bool {
        let mut sessions = self.active_bidi_sessions.lock().await;
        let Some(session) = sessions.get(session_key) else {
            return false;
        };
        let _ = session.input_tx.send(to_rpc_message(request)).await;
        if request.end_of_stream {
            sessions.remove(session_key);
        }
        true
    }

    /// Routes a continuation of an already-open non-bidi multi-message (client-streaming) call
    /// (no `call_metadata`): keeps accumulating, dispatching once the terminal fragment arrives.
    /// Returns `true` if handled — the caller should stop processing this request.
    async fn route_multi_message_continuation(
        &self,
        session_key: &SessionKey,
        request: &RpcRequest,
        outgoing: &mpsc::Sender<(String, RpcResponse)>,
    ) -> bool {
        let mut pending = self.pending_multi_message.lock().await;
        let Some(entry) = pending.get_mut(session_key) else {
            return false;
        };
        entry.messages.push(to_rpc_message(request));
        if !request.end_of_stream {
            return true;
        }
        let entry = pending
            .remove(session_key)
            .expect("just matched via get_mut above");
        drop(pending);
        let (peer, request_id) = (session_key.0.clone(), request.request_id);
        self.spawn_dispatch(
            peer,
            request_id,
            entry.service,
            entry.method,
            entry.messages,
            outgoing.clone(),
        );
        true
    }

    /// Dispatch a fully-collected message list to the bridge in a background task and forward
    /// its response(s). Spawned unconditionally — not just for streaming bodies — because the
    /// handler itself (`bridge.handle_messages`) may block for arbitrarily long (e.g. it might
    /// call back out to the peer that sent this very request over the same duplex channel, and
    /// await that peer's response). Awaiting it inline on the transport's single read loop would
    /// block the very thing that response needs in order to ever arrive — a self-deadlock.
    fn spawn_dispatch(
        &self,
        peer: String,
        request_id: i32,
        service: String,
        method: String,
        messages: Vec<RpcMessage>,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        let bridge = self.bridge.clone();
        tokio::spawn(async move {
            let result = bridge.handle_messages(&service, &method, &messages).await;
            match result {
                Ok(body) => {
                    Self::forward_response_body(request_id, peer, body, outgoing).await;
                }
                Err(status) => {
                    let _ = outgoing
                        .send((peer, Self::error_response(request_id, status)))
                        .await;
                }
            }
        });
    }

    async fn open_bidi_session(
        &self,
        peer: &str,
        request: RpcRequest,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        let request_id = request.request_id;
        let session_key = (peer.to_string(), request_id);
        let meta = request
            .call_metadata
            .clone()
            .expect("opens_bidi_session requires call_metadata");

        let (input_tx, input_rx) = mpsc::channel::<RpcMessage>(64);
        let _ = input_tx.send(to_rpc_message(&request)).await;

        if request.end_of_stream {
            // Single-message call: no continuation will arrive. Don't register bookkeeping —
            // `input_tx` drops at the end of this function, closing `input_rx` once the first
            // message is drained.
        } else {
            self.active_bidi_sessions
                .lock()
                .await
                .insert(session_key, BidiSession { input_tx });
        }

        let bridge = self.bridge.clone();
        let peer_owned = peer.to_string();
        tokio::spawn(async move {
            match bridge
                .start_bidi_stream(&meta.service, &meta.method, input_rx)
                .await
            {
                Ok(BidiStreamOutput { output }) => {
                    Self::forward_response_body(request_id, peer_owned, output, outgoing).await;
                }
                Err(status) => {
                    let _ = outgoing
                        .send((peer_owned, Self::error_response(request_id, status)))
                        .await;
                }
            }
        });
    }

    fn error_response(request_id: i32, status: Status) -> RpcResponse {
        RpcResponse {
            request_id,
            response_message: vec![],
            metadata: None,
            end_of_stream: true,
            error: Some(RpcError {
                code: status.code.as_str().to_string(),
                message: status.message,
                details: HashMap::new(),
            }),
            trailers: None,
        }
    }

    /// Forward a response body onto `outgoing`, tagging the last chunk with `end_of_stream`.
    ///
    /// [`ResponseBody::Streaming`] is always forwarded item-by-item, immediately, as each one is
    /// produced — never looking ahead to see whether a further item exists. A producer may be
    /// real-time-interactive (see bidi's `EchoBidi` in the stdio acceptance tests), emitting its
    /// next item only *after* the peer reacts to the current response; looking one item ahead
    /// would block forever waiting for an item that only shows up once the peer has already
    /// received the one being withheld — a deadlock. Since no item can be tagged
    /// `end_of_stream=true` at send time, a separate, empty, error-free closing frame signals
    /// closure once the channel ends cleanly (see [`crate::client_engine::ClientEngine::on_response`],
    /// which recognizes and doesn't forward this frame as data).
    async fn forward_response_body(
        request_id: i32,
        peer: String,
        body: ResponseBody,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        match body {
            ResponseBody::Complete(chunks) => {
                let len = chunks.len();
                for (i, bytes) in chunks.into_iter().enumerate() {
                    let response = RpcResponse {
                        request_id,
                        response_message: bytes,
                        metadata: None,
                        end_of_stream: i + 1 == len,
                        error: None,
                        trailers: None,
                    };
                    if outgoing.send((peer.clone(), response)).await.is_err() {
                        break;
                    }
                }
            }
            ResponseBody::Streaming(mut rx) => {
                while let Some(item) = rx.recv().await {
                    let (response, is_error) = match item {
                        Ok(bytes) => (
                            RpcResponse {
                                request_id,
                                response_message: bytes,
                                metadata: None,
                                end_of_stream: false,
                                error: None,
                                trailers: None,
                            },
                            false,
                        ),
                        Err(status) => (Self::error_response(request_id, status), true),
                    };
                    if outgoing.send((peer.clone(), response)).await.is_err() {
                        return;
                    }
                    if is_error {
                        // error_response is already terminal (end_of_stream=true) — the stream
                        // ends here, no separate closing frame needed.
                        return;
                    }
                }
                let closing_signal = RpcResponse {
                    request_id,
                    response_message: Vec::new(),
                    metadata: None,
                    end_of_stream: true,
                    error: None,
                    trailers: None,
                };
                let _ = outgoing.send((peer, closing_signal)).await;
            }
        }
    }
}
