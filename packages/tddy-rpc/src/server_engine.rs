//! Transport-agnostic dispatch for incoming RPC requests. Routes decoded `RpcRequest`s into an
//! [`RpcBridge<S>`] and multiplexes concurrent unary/stream/bidi state by `(peer, request_id)` —
//! a peer identifier is required because request ids are only unique per-peer, not globally.
//! Results are published by sending `(peer, response)` pairs into a caller-supplied channel; this
//! engine never touches a transport directly.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::bridge::{BidiStreamOutput, ResponseBody, RpcBridge, RpcService};
use crate::envelope::{CallMetadata, CallOrigin, RpcError, RpcRequest, RpcResponse};
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

/// Unique within one engine: a forward's identity cannot be its request id, because ids are only
/// unique per peer and a peer may reuse one for a later call.
type ForwardId = u64;

/// The background forwards each peer currently has in flight, so all of a peer's forwards can be
/// torn down together once that peer is gone.
#[derive(Default)]
struct PeerForwards {
    next_id: AtomicU64,
    by_peer: Mutex<HashMap<String, HashMap<ForwardId, JoinHandle<()>>>>,
}

impl PeerForwards {
    /// Spawn `forward` attributed to `peer` and remember it until it either finishes on its own or
    /// is aborted by [`Self::abort_all`].
    ///
    /// The handle is registered while the registry is locked, and the task needs that same lock to
    /// deregister itself — so a forward that finishes immediately cannot deregister before it is
    /// registered, which would leave its handle behind forever.
    async fn spawn(
        self: &Arc<Self>,
        peer: String,
        forward: impl Future<Output = ()> + Send + 'static,
    ) {
        let forward_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut by_peer = self.by_peer.lock().await;
        let registry = self.clone();
        let forward_peer = peer.clone();
        let handle = tokio::spawn(async move {
            forward.await;
            registry.forget(&forward_peer, forward_id).await;
        });
        by_peer.entry(peer).or_default().insert(forward_id, handle);
    }

    /// Forget a forward that finished on its own, so a long-lived peer's registry doesn't grow with
    /// one handle per completed call.
    async fn forget(&self, peer: &str, forward_id: ForwardId) {
        let mut by_peer = self.by_peer.lock().await;
        let Some(forwards) = by_peer.get_mut(peer) else {
            return;
        };
        forwards.remove(&forward_id);
        if forwards.is_empty() {
            by_peer.remove(peer);
        }
    }

    /// Abort every forward of `peer` and wait for each to actually finish, so that once this
    /// returns no forward of that peer can publish anything further.
    async fn abort_all(&self, peer: &str) {
        // Scoped so the registry is unlocked before awaiting: an aborted forward may be parked in
        // `forget`, waiting for this very lock.
        let forwards = self.by_peer.lock().await.remove(peer);
        for handle in forwards.into_iter().flatten().map(|(_, handle)| handle) {
            handle.abort();
            // `Err(JoinError::Cancelled)` is the expected outcome of the abort above.
            let _ = handle.await;
        }
    }
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
    peer_forwards: Arc<PeerForwards>,
}

impl<S: RpcService> ServerEngine<S> {
    pub fn new(service: S) -> Self {
        Self {
            bridge: Arc::new(RpcBridge::new(service)),
            active_bidi_sessions: Mutex::new(HashMap::new()),
            pending_multi_message: Mutex::new(HashMap::new()),
            peer_forwards: Arc::new(PeerForwards::default()),
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

        if request.call_metadata.is_some() {
            log::info!(
                "[rpc] engine dispatch service={:?} method={:?} opens_bidi_session={} is_bidi={} end_of_stream={}",
                service,
                method,
                opens_bidi_session,
                self.bridge.is_bidi_stream(service, method),
                request.end_of_stream,
            );
        }

        if opens_bidi_session {
            log::info!(
                "[rpc] engine opening bidi session for {}/{}",
                service,
                method
            );
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
            CallOrigin::of(&request),
            service.to_string(),
            method.to_string(),
            vec![message],
            outgoing,
        )
        .await;
    }

    /// Release everything the engine holds for `peer`, once that peer has gone away.
    ///
    /// Retained state is dropped first — dropping a bidi session's `input_tx` closes its handler's
    /// input, which is how the handler learns to stop — and then the peer's in-flight forwards are
    /// aborted and awaited. Awaiting them is what makes the guarantee observable: when this returns,
    /// nothing further will be published for `peer` and nothing is retained for it.
    ///
    /// Nothing is published to the departed peer, not even a closing or error frame: no one is left
    /// to read it, and publishing for a peer that is gone is exactly the waste being removed.
    pub async fn on_peer_disconnected(&self, peer: &str) {
        log::info!(
            "[rpc] engine releasing state held for departed peer {}",
            peer
        );
        self.active_bidi_sessions
            .lock()
            .await
            .retain(|(session_peer, _), _| session_peer != peer);
        self.pending_multi_message
            .lock()
            .await
            .retain(|(session_peer, _), _| session_peer != peer);
        self.peer_forwards.abort_all(peer).await;
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
        let peer = session_key.0.clone();
        // The continuation carries no `call_metadata` — only the opening frame does — so the call
        // is named from the accumulating entry, which recorded it when the call opened.
        let origin = CallOrigin {
            request_id: request.request_id,
            client_epoch: request.client_epoch,
            call_metadata: Some(CallMetadata {
                service: entry.service.clone(),
                method: entry.method.clone(),
            }),
        };
        self.spawn_dispatch(
            peer,
            origin,
            entry.service,
            entry.method,
            entry.messages,
            outgoing.clone(),
        )
        .await;
        true
    }

    /// Dispatch a fully-collected message list to the bridge in a background task and forward
    /// its response(s). Spawned unconditionally — not just for streaming bodies — because the
    /// handler itself (`bridge.handle_messages`) may block for arbitrarily long (e.g. it might
    /// call back out to the peer that sent this very request over the same duplex channel, and
    /// await that peer's response). Awaiting it inline on the transport's single read loop would
    /// block the very thing that response needs in order to ever arrive — a self-deadlock.
    ///
    /// The task is attributed to `peer` so [`Self::on_peer_disconnected`] can end it: without that,
    /// a server-streaming forward goes on pumping items into `outgoing` for a client that will never
    /// read them, for as long as its producer keeps producing.
    async fn spawn_dispatch(
        &self,
        peer: String,
        origin: CallOrigin,
        service: String,
        method: String,
        messages: Vec<RpcMessage>,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        let bridge = self.bridge.clone();
        let forward_peer = peer.clone();
        self.peer_forwards
            .spawn(peer, async move {
                let result = bridge.handle_messages(&service, &method, &messages).await;
                match result {
                    Ok(body) => {
                        Self::forward_response_body(origin, forward_peer, body, outgoing).await;
                    }
                    Err(status) => {
                        let _ = outgoing
                            .send((forward_peer, Self::error_response(&origin, status)))
                            .await;
                    }
                }
            })
            .await;
    }

    async fn open_bidi_session(
        &self,
        peer: &str,
        request: RpcRequest,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        let request_id = request.request_id;
        let session_key = (peer.to_string(), request_id);
        let origin = CallOrigin::of(&request);
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
        self.peer_forwards
            .spawn(peer.to_string(), async move {
                match bridge
                    .start_bidi_stream(&meta.service, &meta.method, input_rx)
                    .await
                {
                    Ok(BidiStreamOutput { output }) => {
                        Self::forward_response_body(origin, peer_owned, output, outgoing).await;
                    }
                    Err(status) => {
                        let _ = outgoing
                            .send((peer_owned, Self::error_response(&origin, status)))
                            .await;
                    }
                }
            })
            .await;
    }

    fn error_response(origin: &CallOrigin, status: Status) -> RpcResponse {
        RpcResponse {
            request_id: origin.request_id,
            response_message: vec![],
            metadata: None,
            end_of_stream: true,
            error: Some(RpcError {
                code: status.code.as_str().to_string(),
                message: status.message,
                details: HashMap::new(),
            }),
            trailers: None,
            client_epoch: origin.client_epoch,
            call_metadata: origin.call_metadata.clone(),
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
        origin: CallOrigin,
        peer: String,
        body: ResponseBody,
        outgoing: mpsc::Sender<(String, RpcResponse)>,
    ) {
        match body {
            ResponseBody::Complete(chunks) => {
                let len = chunks.len();
                for (i, bytes) in chunks.into_iter().enumerate() {
                    let response = RpcResponse {
                        request_id: origin.request_id,
                        response_message: bytes,
                        metadata: None,
                        end_of_stream: i + 1 == len,
                        error: None,
                        trailers: None,
                        client_epoch: origin.client_epoch,
                        call_metadata: origin.call_metadata.clone(),
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
                                request_id: origin.request_id,
                                response_message: bytes,
                                metadata: None,
                                end_of_stream: false,
                                error: None,
                                trailers: None,
                                client_epoch: origin.client_epoch,
                                call_metadata: origin.call_metadata.clone(),
                            },
                            false,
                        ),
                        Err(status) => (Self::error_response(&origin, status), true),
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
                    request_id: origin.request_id,
                    response_message: Vec::new(),
                    metadata: None,
                    end_of_stream: true,
                    error: None,
                    trailers: None,
                    client_epoch: origin.client_epoch,
                    call_metadata: origin.call_metadata.clone(),
                };
                let _ = outgoing.send((peer, closing_signal)).await;
            }
        }
    }
}
