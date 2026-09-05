//! Many concurrent, independently addressed webview connections.
//!
//! [`WebviewRpcHost`](crate::WebviewRpcHost) hosts one service for a single webview at a time: it
//! holds `Option<Connection>`, and `connect` abandons whatever was there. That was right while a
//! page had exactly one thing to reach. It is what stops a page from holding a connection to the
//! daemon *and* one per attached session.
//!
//! This host keeps a **map** instead, keyed by the client epoch the page already stamps its frames
//! with. Everything else about the design survives unchanged, which is what makes the change
//! tractable:
//!
//! - the engine is already peer-keyed (`webview-{epoch}`), and releases a peer's state as a unit;
//! - backpressure is already per connection — one bounded queue and one drain task each;
//! - the epoch already routes frames, so there is **no frame-format change** and no protocol
//!   version to negotiate.
//!
//! What narrows is the meaning of [`FrameError::StaleConnection`](crate::FrameError): from "a page
//! that was replaced" to "a connection this host does not have".

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tddy_rpc::envelope::{self, RpcResponse};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::RpcService;
use tokio::sync::{mpsc, Mutex};

use crate::host::FrameSink;
use crate::target::{ConnectError, ConnectionTarget, RosterResolver};
use crate::FrameError;

/// Bounded, and bounded **per connection**: a page that stops reading one of its IPC channels
/// applies backpressure to that connection's engine alone. One queue shared by every connection
/// would let a single unread channel stall all the others, which is precisely what a page holding
/// a daemon connection and one per session cannot afford.
const RESPONSE_QUEUE_CAPACITY: usize = 256;

/// One connection's dispatch. The roster is a trait object because a [`RosterResolver`] picks it at
/// runtime — the host cannot name the type it will be handed.
type ConnectionEngine = ServerEngine<Arc<dyn RpcService>>;

/// Every connection this host holds, keyed by the client epoch its frames carry.
type Connections = HashMap<u32, Connection>;

/// Distinguishes one connection from the next opened under the same epoch.
///
/// The map is keyed by epoch and the engine peer is derived from the epoch alone, so neither can
/// tell a connection apart from its successor — and a departing connection must never tear down the
/// one that took its place.
type ConnectionId = u64;

/// Hosts many webview connections at once, each reaching the roster its target names.
pub struct MultiConnectionHost<R: RosterResolver> {
    resolver: R,
    /// Shared with every connection's drain task, because a departed page is first noticed by the
    /// task publishing to it, and that is therefore where the connection has to be removed.
    connections: Arc<Mutex<Connections>>,
    /// Hands out the next [`ConnectionId`].
    next_connection_id: AtomicU64,
}

/// One webview connection: the roster it reaches, the sink its responses go to, and the engine peer
/// it is known by.
struct Connection {
    /// Its own identity, so a drain task can tell whether the map still holds *this* connection.
    id: ConnectionId,
    /// Its own engine, over the roster its target resolved to. Two connections may serve different
    /// rosters, and releasing one must leave what the others have in flight untouched.
    engine: Arc<ConnectionEngine>,
    peer: String,
    sink: Arc<dyn FrameSink>,
    responses: mpsc::Sender<(String, RpcResponse)>,
}

impl<R: RosterResolver> MultiConnectionHost<R> {
    /// Host connections whose targets `resolver` resolves.
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            connections: Arc::new(Mutex::new(Connections::new())),
            next_connection_id: AtomicU64::new(0),
        }
    }

    /// Open a connection to `target`, publishing its responses onto `sink`.
    ///
    /// `client_epoch` is the connection's identity: the page mints one per transport and stamps
    /// every frame with it, so it is what routes a frame to the right connection.
    ///
    /// Opening a connection **does not disturb any other**. Calls in flight on the page's other
    /// connections are unaffected — which is the whole point, and the opposite of what
    /// `WebviewRpcHost::connect` does.
    pub async fn connect(
        &self,
        target: ConnectionTarget,
        sink: Arc<dyn FrameSink>,
        client_epoch: u32,
    ) -> Result<(), ConnectError> {
        // Resolved before anything is registered, so a refused target leaves no connection behind
        // for the page to remember to release.
        let Some(roster) = self.resolver.roster_for(&target) else {
            return Err(ConnectError::NoSuchTarget { target });
        };

        // Held across the check and the insert, so two connects racing on one epoch cannot both
        // find it free.
        let mut connections = self.connections.lock().await;
        if connections.contains_key(&client_epoch) {
            // The connection already registered under this epoch keeps serving: it is the caller of
            // *this* connect that reused an epoch, and evicting the incumbent would punish the
            // calls it has in flight for someone else's mistake.
            return Err(ConnectError::EpochInUse { client_epoch });
        }

        let id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
        let peer = peer_for(client_epoch);
        let engine = Arc::new(ServerEngine::new(roster));
        let (responses, response_rx) = mpsc::channel(RESPONSE_QUEUE_CAPACITY);
        tokio::spawn(drain_responses(
            engine.clone(),
            self.connections.clone(),
            id,
            client_epoch,
            sink.clone(),
            response_rx,
        ));
        connections.insert(
            client_epoch,
            Connection {
                id,
                engine,
                peer,
                sink,
                responses,
            },
        );
        Ok(())
    }

    /// Release the connection registered under `client_epoch`.
    ///
    /// Idempotent. The engine drops that peer's state, the forwards still publishing for it abort,
    /// and the sink is closed. **Every other connection keeps serving.**
    ///
    /// Sessions come and go far more often than pages do, so without this every attach would leak a
    /// host-side peer — a leak the single-slot host never had, because there was only ever one.
    pub async fn disconnect(&self, client_epoch: u32) {
        let released = {
            let mut connections = self.connections.lock().await;
            connections.remove(&client_epoch)
        };
        // Released with the map unlocked: the engine waits for this connection's forwards to stop,
        // and a forward parked on a full response queue is only freed by its drain task — which
        // needs this same lock to report a departed page.
        if let Some(connection) = released {
            release(connection).await;
        }
    }

    /// Release **every** connection a page owned.
    ///
    /// A page reload used to reap the previous page's connection automatically, because there was
    /// one slot and `connect` overwrote it. With a map that is no longer implicit, and a leaked
    /// per-session connection on every reload is exactly what this exists to prevent.
    pub async fn disconnect_all(&self) {
        let released: Vec<Connection> = {
            let mut connections = self.connections.lock().await;
            connections
                .drain()
                .map(|(_, connection)| connection)
                .collect()
        };
        for connection in released {
            release(connection).await;
        }
    }

    /// Decode and dispatch one request frame, routing it by the epoch it carries.
    pub async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError> {
        let request = envelope::decode_request(frame).map_err(FrameError::Malformed)?;

        // Publishing takes priority over accepting: hand the runtime a turn so responses the
        // engines have already produced reach their sinks before another call is dispatched. A
        // failed publish is the only way this host ever learns a page is gone, so a host driven
        // purely by inbound frames would otherwise go on dispatching calls for a page that left.
        tokio::task::yield_now().await;

        // The map is only borrowed long enough to learn which connection answers this call:
        // `on_request` runs a handler that may itself block on a drain task making room in a
        // bounded response queue, and that drain task needs this same lock to report its page gone.
        let (engine, peer, responses) = {
            let connections = self.connections.lock().await;
            let Some(connection) = connections.get(&request.client_epoch) else {
                return Err(refuse_unknown_epoch(&connections, request.client_epoch));
            };
            (
                connection.engine.clone(),
                connection.peer.clone(),
                connection.responses.clone(),
            )
        };

        log::debug!(
            "[tauri-rpc] {peer} <- request {} {}/{}",
            request.request_id,
            request
                .call_metadata
                .as_ref()
                .map(|m| m.service.as_str())
                .unwrap_or("(continuation)"),
            request
                .call_metadata
                .as_ref()
                .map(|m| m.method.as_str())
                .unwrap_or(""),
        );
        engine.on_request(&peer, request, responses).await;
        Ok(())
    }

    /// How many connections are open. Diagnostics and tests.
    pub async fn connection_count(&self) -> usize {
        self.connections.lock().await.len()
    }
}

/// Why a frame naming no connection is refused rather than dispatched onto some other one: its
/// answer would go out on that connection's sink, be dropped there for the epoch mismatch, and the
/// caller that sent it would wait for an answer that can never arrive.
///
/// [`FrameError::StaleConnection`] is the more useful of the two refusals, because it names the
/// connection the frame missed — but `connected` is a *single* epoch, and a host serving many has
/// no single "the connected one". It is therefore reported only when there is exactly one
/// connection to name, where doing so is the truth. Otherwise the honest report is
/// [`FrameError::NotConnected`]: nothing here answers for that epoch, and the page has to open a
/// connection before anything can.
fn refuse_unknown_epoch(connections: &Connections, frame: u32) -> FrameError {
    let mut epochs = connections.keys();
    match (epochs.next(), epochs.next()) {
        (Some(&connected), None) => FrameError::StaleConnection { connected, frame },
        _ => FrameError::NotConnected,
    }
}

/// Give up on `connection`: its engine releases everything it holds for that peer — which aborts
/// the forwards still publishing for it — and the sink is told nothing further is coming. Dropping
/// the connection drops the last sender of its response queue, which is what ends the task draining
/// onto that sink.
async fn release(connection: Connection) {
    connection
        .engine
        .on_peer_disconnected(&connection.peer)
        .await;
    connection.sink.close();
}

/// Publish every response one connection's engine produces onto that connection's sink, one frame
/// per response, until its response queue closes (the connection was released) or its page is gone.
async fn drain_responses(
    engine: Arc<ConnectionEngine>,
    connections: Arc<Mutex<Connections>>,
    id: ConnectionId,
    client_epoch: u32,
    sink: Arc<dyn FrameSink>,
    mut responses: mpsc::Receiver<(String, RpcResponse)>,
) {
    let peer = peer_for(client_epoch);
    while let Some((_peer, response)) = responses.recv().await {
        let request_id = response.request_id;
        let end_of_stream = response.end_of_stream;
        let frame = match envelope::encode_response(response) {
            Ok(frame) => frame,
            Err(reason) => {
                log::error!(
                    "[tauri-rpc] dropping an unencodable response to request {request_id} of {peer}: {reason}"
                );
                continue;
            }
        };
        log::debug!(
            "[tauri-rpc] {peer} -> response {request_id} ({} bytes, end_of_stream={})",
            frame.len(),
            end_of_stream,
        );
        if sink.send(frame).is_err() {
            release_departed_connection(&engine, &connections, id, client_epoch, &peer).await;
            return;
        }
    }
}

/// Stop serving a connection whose sink reported its page gone: remove it so the next frame stamped
/// with its epoch is refused, and release the engine state its calls hold.
///
/// Only this connection is removed, and only while the map still holds it. An epoch is a key, not a
/// generation: a connection opened under the same epoch after this one departed would otherwise be
/// torn down by its predecessor. Every other connection is left untouched either way — that a page
/// lost one channel says nothing about the rest.
async fn release_departed_connection(
    engine: &ConnectionEngine,
    connections: &Mutex<Connections>,
    id: ConnectionId,
    client_epoch: u32,
    peer: &str,
) {
    let departed = {
        let mut connections = connections.lock().await;
        match connections.get(&client_epoch) {
            Some(current) if current.id == id => connections.remove(&client_epoch),
            _ => None,
        }
    };
    if let Some(departed) = departed {
        departed.sink.close();
    }
    engine.on_peer_disconnected(peer).await;
}

/// Name the engine peer for a connection, so the state it holds can be released as a unit once that
/// connection goes away. The same naming the single-connection host uses: an epoch identifies a
/// connection on either host, and nothing serves both.
fn peer_for(client_epoch: u32) -> String {
    format!("webview-{client_epoch}")
}
