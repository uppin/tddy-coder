//! The host side of the webview-IPC flavour: decode request frames, dispatch them through a
//! [`ServerEngine`], and publish every resulting response frame onto the connected sink.

use std::sync::Arc;

use tddy_rpc::envelope::{self, RpcResponse};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::RpcService;
use tokio::sync::{mpsc, Mutex};

/// Bounded, so a webview that stops reading its IPC channel applies backpressure to the engine
/// instead of letting undelivered responses accumulate.
const RESPONSE_QUEUE_CAPACITY: usize = 256;

/// Where the host writes encoded `RpcResponse` frames for the webview to read.
///
/// One implementation per host application (a Tauri app wraps `tauri::ipc::Channel`); the host
/// itself never learns which.
pub trait FrameSink: Send + Sync + 'static {
    /// Publish one encoded `RpcResponse` frame. Returns [`SinkClosed`] once the webview is gone.
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed>;

    /// Called when the host will publish nothing further on this sink — the page reconnected, or
    /// the window closed. A sink that signals completion downstream does it here.
    fn close(&self);
}

/// The sink's peer is gone; nothing further can be published on it.
#[derive(Debug, PartialEq, Eq)]
pub struct SinkClosed;

/// Why a request frame was not accepted.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A frame arrived before any webview registered a sink, so no response could be delivered.
    NotConnected,
    /// The frame's bytes are not a decodable `RpcRequest`.
    Malformed(String),
    /// The frame belongs to a page connection this host has already replaced.
    ///
    /// Dispatching it anyway would answer it onto the *current* page's channel, where the epoch
    /// does not match and the response is dropped — so the caller that sent it would wait for an
    /// answer that can never arrive. Refusing says so instead.
    StaleConnection {
        /// The epoch of the page currently connected.
        connected: u32,
        /// The epoch the refused frame carried.
        frame: u32,
    },
}

/// Hosts `S` for a single webview at a time.
///
/// A page reload mints a fresh client epoch and re-registers its sink. Each connection is a
/// distinct engine peer, which is what lets [`Self::connect`] abandon everything the previous
/// page opened: a request id restarts at 1 on reload while the engine may still be streaming for
/// ids the new page is about to hand out again.
pub struct WebviewRpcHost<S: RpcService> {
    engine: Arc<ServerEngine<S>>,
    /// Shared with the task draining responses onto the connected sink, which is where a departed
    /// webview is first noticed and therefore where the connection has to be cleared.
    connection: Arc<Mutex<Option<Connection>>>,
}

/// One webview connection: the sink its responses go to, and the engine peer it is known by.
struct Connection {
    /// The page connection this is, as the webview named it in `connect`.
    client_epoch: u32,
    peer: String,
    sink: Arc<dyn FrameSink>,
    responses: mpsc::Sender<(String, RpcResponse)>,
}

impl<S: RpcService> WebviewRpcHost<S> {
    /// Host `service`. No webview is connected until [`Self::connect`] is called.
    pub fn new(service: S) -> Self {
        Self {
            engine: Arc::new(ServerEngine::new(service)),
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// Register `sink` as the response channel for the page identified by `client_epoch`,
    /// abandoning every call the previous page opened.
    ///
    /// The whole swap happens under the connection lock, so a request frame that arrives during a
    /// reload is either answered by the page that sent it or waits for the page replacing it —
    /// never dispatched into a connection that is halfway through being replaced.
    pub async fn connect(&self, sink: Arc<dyn FrameSink>, client_epoch: u32) {
        let peer = Connection::peer_for(client_epoch);
        let (responses, response_rx) = mpsc::channel(RESPONSE_QUEUE_CAPACITY);
        let mut slot = self.connection.lock().await;

        if let Some(previous) = slot.take() {
            self.abandon(previous).await;
        }

        tokio::spawn(drain_responses(
            self.engine.clone(),
            self.connection.clone(),
            peer.clone(),
            sink.clone(),
            response_rx,
        ));
        *slot = Some(Connection {
            client_epoch,
            peer,
            sink,
            responses,
        });
    }

    /// Decode and dispatch one request frame from the connected webview.
    pub async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError> {
        let request = envelope::decode_request(frame).map_err(FrameError::Malformed)?;

        // Publishing takes priority over accepting: hand the runtime a turn so responses the
        // engine has already produced reach the sink before another call is dispatched. A failed
        // publish is the only way this host ever learns its page is gone, so a host driven purely
        // by inbound frames would otherwise go on dispatching calls for a page that left.
        tokio::task::yield_now().await;

        // The connection is only borrowed long enough to learn where this call's answers go:
        // `on_request` runs a handler that may itself block on the drain task making room in the
        // response queue, and that task needs this same lock to report a departed page.
        let (peer, responses) = {
            let slot = self.connection.lock().await;
            let connection = slot.as_ref().ok_or(FrameError::NotConnected)?;
            // A frame minted by a page this host has already replaced is refused rather than
            // dispatched: its answer would go to the current page's channel and be dropped there
            // for the same epoch mismatch, and the caller would wait for it forever. A caller that
            // is told is a caller that can fail.
            if request.client_epoch != connection.client_epoch {
                return Err(FrameError::StaleConnection {
                    connected: connection.client_epoch,
                    frame: request.client_epoch,
                });
            }
            (connection.peer.clone(), connection.responses.clone())
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
        self.engine.on_request(&peer, request, responses).await;
        Ok(())
    }

    /// Give up on `connection`: the engine releases everything it holds for that peer — which
    /// aborts the forwards still publishing for it — and the sink is told nothing further is
    /// coming. Dropping the connection drops the last sender of its response queue, which is what
    /// ends the task draining onto that sink.
    async fn abandon(&self, connection: Connection) {
        self.engine.on_peer_disconnected(&connection.peer).await;
        connection.sink.close();
    }
}

/// Publish every response the engine produces for `peer` onto `sink`, one frame per response,
/// until the response queue closes (the connection was abandoned) or the webview is gone.
async fn drain_responses<S: RpcService>(
    engine: Arc<ServerEngine<S>>,
    connection: Arc<Mutex<Option<Connection>>>,
    peer: String,
    sink: Arc<dyn FrameSink>,
    mut responses: mpsc::Receiver<(String, RpcResponse)>,
) {
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
            release_departed_peer(&engine, &connection, &peer).await;
            return;
        }
    }
}

/// Stop serving a page whose sink reported its peer gone: clear it from the connection slot so the
/// next request frame is refused, and release the engine state its calls hold.
///
/// The slot is only cleared if it still holds this peer — a page that reconnected in the meantime
/// already replaced this connection, and must not be torn down by the departure of the one before
/// it.
async fn release_departed_peer<S: RpcService>(
    engine: &ServerEngine<S>,
    connection: &Mutex<Option<Connection>>,
    peer: &str,
) {
    let departed = {
        let mut slot = connection.lock().await;
        match slot.as_ref() {
            Some(current) if current.peer == peer => slot.take(),
            _ => None,
        }
    };
    if let Some(departed) = departed {
        departed.sink.close();
    }
    engine.on_peer_disconnected(peer).await;
}

impl Connection {
    /// Name the engine peer for a page connection, so per-connection state can be released as a
    /// unit when that page goes away.
    fn peer_for(client_epoch: u32) -> String {
        format!("webview-{client_epoch}")
    }
}
