//! Framed duplex endpoint: reads/decodes frames from one side of a pipe, dispatches `Request`
//! frames to a hosted [`ServerEngine`] and `Response` frames to the local [`StdioRpcClient`]'s
//! `ClientEngine`, and funnels every outgoing frame (client requests and server responses alike)
//! through one writer task that owns the write half — so requests and responses never interleave
//! mid-frame on the wire.

use std::sync::Arc;

use tddy_rpc::envelope::{self, RpcResponse};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::transport::{encode_frame, FrameDecoder, FrameKind};
use tddy_rpc::RpcService;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::StdioRpcClient;

/// A stdio pipe pair always has exactly one peer per end — unlike LiveKit's multi-participant
/// room model, no multi-peer multiplexing is needed for this transport.
const PEER: &str = "peer";

const READ_BUFFER_SIZE: usize = 8192;
/// Bounded so a slow peer applies backpressure rather than letting memory grow unbounded.
const OUTGOING_QUEUE_CAPACITY: usize = 256;

/// An RPC endpoint over one framed duplex byte channel: hosts `service` for inbound requests
/// from the peer, and returns a client for calling into the peer — both over the same channel.
pub struct StdioEndpoint<S: RpcService> {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
    /// Encoded outgoing frames, written to `writer` by the single writer task in [`Self::run`].
    /// Fed by the client directly (requests) and by the response-drain task (responses).
    frame_rx: mpsc::Receiver<Vec<u8>>,
    frame_tx: mpsc::Sender<Vec<u8>>,
    response_rx: mpsc::Receiver<(String, RpcResponse)>,
    response_tx: mpsc::Sender<(String, RpcResponse)>,
    server: Arc<ServerEngine<S>>,
    client: Arc<StdioRpcClient>,
}

impl<S: RpcService> StdioEndpoint<S> {
    fn new(
        reader: Box<dyn AsyncRead + Unpin + Send>,
        writer: Box<dyn AsyncWrite + Unpin + Send>,
        service: S,
    ) -> (Arc<StdioRpcClient>, Self) {
        let (frame_tx, frame_rx) = mpsc::channel(OUTGOING_QUEUE_CAPACITY);
        let (response_tx, response_rx) = mpsc::channel(OUTGOING_QUEUE_CAPACITY);
        let client = StdioRpcClient::new(frame_tx.clone());
        let endpoint = Self {
            reader,
            writer,
            frame_rx,
            frame_tx,
            response_rx,
            response_tx,
            server: Arc::new(ServerEngine::new(service)),
            client: client.clone(),
        };
        (client, endpoint)
    }

    /// Wrap this process's own stdin/stdout, hosting `service` for inbound requests from the
    /// peer that spawned this process. Returns a client for calling back into that peer.
    pub fn from_process_stdio(service: S) -> (Arc<StdioRpcClient>, Self) {
        Self::new(
            Box::new(tokio::io::stdin()),
            Box::new(tokio::io::stdout()),
            service,
        )
    }

    /// Wrap an already-open duplex byte channel (e.g. pipes obtained from a child process spawned
    /// by something other than [`spawn_child_endpoint`] — a jailed/sandboxed spawn that needs
    /// platform-specific process creation `spawn_child_endpoint`'s plain `tokio::process::Command`
    /// can't express). The caller owns spawning the child and converting its stdio into async
    /// `AsyncRead`/`AsyncWrite` handles (e.g. via `tokio::net::unix::pipe` wrapping raw fds from a
    /// blocking `std::process::Child`); this just hosts `service` over them like any other
    /// transport. Returns a client for calling into that peer.
    pub fn from_duplex(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
        service: S,
    ) -> (Arc<StdioRpcClient>, Self) {
        Self::new(Box::new(reader), Box::new(writer), service)
    }

    pub(crate) fn from_child_stdio(
        stdin: ChildStdin,
        stdout: ChildStdout,
        service: S,
    ) -> (Arc<StdioRpcClient>, Self) {
        Self::new(Box::new(stdout), Box::new(stdin), service)
    }

    /// Run the endpoint's read/dispatch/write loop. Returns once the channel closes (EOF or a
    /// write failure).
    pub async fn run(self) {
        let Self {
            mut reader,
            mut writer,
            mut frame_rx,
            frame_tx,
            mut response_rx,
            response_tx,
            server,
            client,
        } = self;

        // Response-drain task: encodes server responses and forwards them into the same
        // outgoing frame queue the client writes requests into.
        let response_drain: JoinHandle<()> = tokio::spawn(async move {
            while let Some((_peer, response)) = response_rx.recv().await {
                if let Ok(payload) = envelope::encode_response(response) {
                    let frame = encode_frame(FrameKind::Response, &payload);
                    if frame_tx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Writer task: the sole owner of the write half. Flushing after every frame matters here
        // — unlike a `BufWriter`, `write_all` alone doesn't guarantee the peer's `read` observes
        // a complete frame promptly, and a peer's own read loop may itself be blocked awaiting a
        // response this write carries (see `ServerEngine::on_request`'s doc comment).
        let writer_task: JoinHandle<()> = tokio::spawn(async move {
            while let Some(frame) = frame_rx.recv().await {
                if writer.write_all(&frame).await.is_err() {
                    break;
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
        });

        let mut decoder = FrameDecoder::new();
        let mut buf = [0u8; READ_BUFFER_SIZE];
        loop {
            let bytes_read = match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            decoder.feed(&buf[..bytes_read]);
            while let Some((kind, payload)) = decoder.next_frame() {
                match kind {
                    FrameKind::Request => {
                        if let Ok(request) = envelope::decode_request(&payload) {
                            server.on_request(PEER, request, response_tx.clone()).await;
                        }
                    }
                    FrameKind::Response => {
                        if let Ok(response) = envelope::decode_response(&payload) {
                            client.deliver_response(response).await;
                        }
                    }
                }
            }
        }

        drop(response_tx);
        let _ = response_drain.await;
        writer_task.abort();
    }
}

/// A client for calling into a spawned child process, which can also call back into the service
/// hosted for it over the same pipe pair. Dropping this kills the child and stops its endpoint
/// loop.
pub struct ChildEndpoint {
    pub client: Arc<StdioRpcClient>,
    child: Child,
    run_handle: JoinHandle<()>,
}

impl Drop for ChildEndpoint {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.run_handle.abort();
    }
}

/// Spawn `command` with piped stdio, wire its stdin/stdout as a framed RPC channel, and host
/// `service` for inbound requests *from* the child (reverse calls). Returns a [`ChildEndpoint`]
/// whose `client` calls *into* the child — both directions work concurrently over the one pipe
/// pair.
pub async fn spawn_child_endpoint<S: RpcService>(
    mut command: Command,
    service: S,
) -> std::io::Result<ChildEndpoint> {
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = command.spawn()?;
    // Guaranteed present: Stdio::piped() was just configured above.
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");

    let (client, endpoint) = StdioEndpoint::from_child_stdio(stdin, stdout, service);
    let run_handle = tokio::spawn(endpoint.run());

    Ok(ChildEndpoint {
        client,
        child,
        run_handle,
    })
}
