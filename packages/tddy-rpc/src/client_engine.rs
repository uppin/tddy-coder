//! Transport-agnostic request-id correlation for outgoing RPC calls. Any transport (LiveKit,
//! stdio, ...) feeds decoded `RpcResponse`s into [`ClientEngine::on_response`]; callers get back
//! the same oneshot/mpsc handles regardless of which transport eventually delivers the bytes.

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot};

use crate::envelope::{CallMetadata, RpcRequest, RpcResponse};
use crate::status::{Code, Status};

/// A pending call: the sender to resolve, plus the call it was opened for. The metadata is kept so
/// an arriving response can be checked against the call actually holding the id.
struct Pending<T> {
    call: CallMetadata,
    sender: T,
}

type PendingUnary = HashMap<i32, Pending<oneshot::Sender<Result<Vec<u8>, Status>>>>;
type PendingStreams = HashMap<i32, Pending<mpsc::Sender<Result<Vec<u8>, Status>>>>;

/// Request ids are allocated **process-globally**, not per engine. Several `ClientEngine`s can
/// share one transport's inbound stream — e.g. multiple `RpcClient`s on one LiveKit room, even
/// several targeting the same peer (`forward_to_peer` builds one per call). Every client sees that
/// stream's responses, so a per-engine counter (each starting at 1) would hand out colliding ids
/// and one engine's response would resolve another's pending call. A global counter keeps every
/// in-flight request id unique across the process, so a response matches only the call that owns it.
static NEXT_REQUEST_ID: AtomicI32 = AtomicI32::new(1);

/// Distinguishes one client connection from the next. Seeded from `RandomState` (OS-seeded per
/// process) mixed with the wall clock and a process-local counter, so neither two connections in
/// one process nor a process and its own restart collide. Never 0 — a zero epoch on the wire means
/// the field was absent.
fn mint_client_epoch() -> u32 {
    static NEXT_SEED: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(NEXT_SEED.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    match (hasher.finish() >> 32) as u32 {
        0 => 1,
        epoch => epoch,
    }
}

fn a_call(service: &str, method: &str) -> CallMetadata {
    CallMetadata {
        service: service.to_string(),
        method: method.to_string(),
    }
}

/// Correlates outgoing requests with their responses by `request_id`. Owns no transport —
/// callers hand it decoded envelopes and get back handles to await/stream from.
pub struct ClientEngine {
    local_identity: String,
    /// Identifies this client connection. Stamped on every outgoing request and required to match
    /// on every incoming response, so a frame minted for a previous connection can never resolve a
    /// call of this one. See [`crate::envelope::CallOrigin`].
    client_epoch: u32,
    pending_unary: Mutex<PendingUnary>,
    pending_streams: Mutex<PendingStreams>,
}

impl ClientEngine {
    /// A new client connection, with a freshly minted [`Self::client_epoch`].
    pub fn new(local_identity: impl Into<String>) -> Self {
        Self::with_client_epoch(local_identity, mint_client_epoch())
    }

    /// A client connection with an explicit epoch. Prefer [`Self::new`]; this exists for callers
    /// that must reproduce a specific connection identity.
    pub fn with_client_epoch(local_identity: impl Into<String>, client_epoch: u32) -> Self {
        Self {
            local_identity: local_identity.into(),
            client_epoch,
            pending_unary: Mutex::new(HashMap::new()),
            pending_streams: Mutex::new(HashMap::new()),
        }
    }

    pub fn local_identity(&self) -> &str {
        &self.local_identity
    }

    /// This connection's epoch, carried by every request it builds.
    pub fn client_epoch(&self) -> u32 {
        self.client_epoch
    }

    fn next_id(&self) -> i32 {
        NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
    }

    /// Register a pending unary call for `service`/`method` and return its request id and resolver.
    pub fn register_unary(
        &self,
        service: &str,
        method: &str,
    ) -> (i32, oneshot::Receiver<Result<Vec<u8>, Status>>) {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();
        self.pending_unary
            .lock()
            .expect("pending_unary mutex poisoned")
            .insert(
                id,
                Pending {
                    call: a_call(service, method),
                    sender: tx,
                },
            );
        (id, rx)
    }

    /// Register a pending streaming call (server-stream or bidi) for `service`/`method` and return
    /// its request id and receiver.
    pub fn register_stream(
        &self,
        service: &str,
        method: &str,
    ) -> (i32, mpsc::Receiver<Result<Vec<u8>, Status>>) {
        let id = self.next_id();
        let (tx, rx) = mpsc::channel(32);
        self.pending_streams
            .lock()
            .expect("pending_streams mutex poisoned")
            .insert(
                id,
                Pending {
                    call: a_call(service, method),
                    sender: tx,
                },
            );
        (id, rx)
    }

    /// Build the first `RpcRequest` for a call, carrying `call_metadata`.
    pub fn build_request(
        &self,
        request_id: i32,
        service: &str,
        method: &str,
        payload: Vec<u8>,
        end_of_stream: bool,
    ) -> RpcRequest {
        RpcRequest {
            request_id,
            request_message: payload,
            call_metadata: Some(CallMetadata {
                service: service.to_string(),
                method: method.to_string(),
            }),
            metadata: None,
            end_of_stream,
            abort: false,
            sender_identity: Some(self.local_identity.clone()),
            client_epoch: self.client_epoch,
        }
    }

    /// Build a continuation `RpcRequest` (no `call_metadata`) for an already-open multi-message
    /// call.
    pub fn build_continuation(
        &self,
        request_id: i32,
        payload: Vec<u8>,
        end_of_stream: bool,
    ) -> RpcRequest {
        RpcRequest {
            request_id,
            request_message: payload,
            call_metadata: None,
            metadata: None,
            end_of_stream,
            abort: false,
            sender_identity: Some(self.local_identity.clone()),
            client_epoch: self.client_epoch,
        }
    }

    /// Register a pending unary call and build its first request in one step.
    pub fn begin_unary(
        &self,
        service: &str,
        method: &str,
        payload: Vec<u8>,
    ) -> (RpcRequest, oneshot::Receiver<Result<Vec<u8>, Status>>) {
        let (id, rx) = self.register_unary(service, method);
        (self.build_request(id, service, method, payload, true), rx)
    }

    /// Register a pending streaming call and build its (single-message) request in one step.
    pub fn begin_stream(
        &self,
        service: &str,
        method: &str,
        payload: Vec<u8>,
    ) -> (RpcRequest, mpsc::Receiver<Result<Vec<u8>, Status>>) {
        let (id, rx) = self.register_stream(service, method);
        (self.build_request(id, service, method, payload, true), rx)
    }

    /// Feed a decoded response: resolves the matching pending unary call, or forwards to the
    /// matching pending stream (closing it on error or `end_of_stream`). No-ops silently on an
    /// unknown `request_id`.
    ///
    /// A terminal stream response with an empty payload and no error is treated as a pure
    /// closing signal — a real-time server (see `ServerEngine::forward_response_body`) can't
    /// know a data item is the last one until after it's already been forwarded, so it signals
    /// closure with a separate, payload-free frame rather than tagging a real item. That frame
    /// closes the stream but is not delivered to the caller as if it were data.
    ///
    /// Stream delivery backpressures (`.send().await`) rather than dropping on a full channel: a
    /// caller that sends a burst of requests before starting to drain responses (a legitimate,
    /// supported pattern — see `tddy-livekit`'s `rpc_scenarios` bidi test) must never silently
    /// lose data just because it hasn't started reading yet.
    pub async fn on_response(&self, response: RpcResponse) {
        let request_id = response.request_id;

        // Attribute the response before touching any pending call. A peer keeps serving streams
        // opened by a connection that has gone away (a browser reload, a process restart) and
        // addresses them to the same transport identity, while request ids restart from scratch —
        // so an id match alone does not mean this response belongs to the call now holding it.
        // Delivering it anyway hands the caller another call's bytes to decode as its own message
        // type, with no error: how a terminal's output came to be displayed as a control lease's
        // holder screen id.
        if response.client_epoch != self.client_epoch {
            log::debug!(
                "[rpc] dropping response request_id={} from client_epoch={} (this connection is {})",
                request_id,
                response.client_epoch,
                self.client_epoch,
            );
            return;
        }
        if !self.answers_the_pending_call(request_id, response.call_metadata.as_ref()) {
            log::debug!(
                "[rpc] dropping response request_id={} answering {:?}, which is not the call holding that id",
                request_id,
                response.call_metadata,
            );
            return;
        }

        let terminal = response.end_of_stream || response.error.is_some();
        let is_pure_closing_signal = response.end_of_stream
            && response.error.is_none()
            && response.response_message.is_empty();
        let result: Result<Vec<u8>, Status> = match response.error {
            Some(err) => Err(Status {
                code: Code::from_str(&err.code),
                message: err.message,
            }),
            None => Ok(response.response_message),
        };

        // Look up (and possibly remove) the pending stream sender, then drop the lock before
        // awaiting `send` — never hold a std::sync::Mutex guard across an await point.
        let stream_sender = {
            let mut streams = self
                .pending_streams
                .lock()
                .expect("pending_streams mutex poisoned");
            let sender = streams
                .get(&request_id)
                .map(|pending| pending.sender.clone());
            if sender.is_some() && terminal {
                streams.remove(&request_id);
            }
            sender
        };

        if let Some(tx) = stream_sender {
            if !is_pure_closing_signal {
                let _ = tx.send(result).await;
            }
            return;
        }

        if let Some(pending) = self
            .pending_unary
            .lock()
            .expect("pending_unary mutex poisoned")
            .remove(&request_id)
        {
            let _ = pending.sender.send(result);
        }
    }

    /// Whether `call` names the same method as the call currently registered under `request_id`.
    ///
    /// An unknown id is accepted here so the existing "no-op on an unknown request_id" behaviour is
    /// unchanged, and a response carrying no metadata is accepted so a continuation frame is never
    /// refused for lacking what only an opening frame carries.
    fn answers_the_pending_call(&self, request_id: i32, call: Option<&CallMetadata>) -> bool {
        let Some(call) = call else {
            return true;
        };
        let expected = {
            let streams = self
                .pending_streams
                .lock()
                .expect("pending_streams mutex poisoned");
            match streams.get(&request_id) {
                Some(pending) => Some(pending.call.clone()),
                None => self
                    .pending_unary
                    .lock()
                    .expect("pending_unary mutex poisoned")
                    .get(&request_id)
                    .map(|pending| pending.call.clone()),
            }
        };
        match expected {
            Some(expected) => expected == *call,
            None => true,
        }
    }
}
