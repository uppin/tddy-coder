//! Terminal service: single VirtualTui-per-connection implementation.
//!
//! `start_virtual_tui_session` is the ONE entry point for creating a VirtualTui
//! streaming session. `TerminalServiceVirtualTui` is the sole `TerminalService`
//! trait impl — used directly for LiveKit (via `TerminalServiceServer` / RpcService)
//! and for gRPC (via the tonic-generated `TerminalServiceServer` from `tonic_terminal`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use tddy_core::ViewConnection;
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_tui::{run_virtual_tui, VIRTUAL_TUI_REPAINT_REQUEST};

use crate::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};

type OutputChunkHook = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// A running VirtualTui session: send keyboard bytes via `input_tx`,
/// receive ANSI output bytes via `output_rx`, signal `shutdown` to stop.
pub struct VirtualTuiSession {
    pub input_tx: mpsc::Sender<Vec<u8>>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    pub shutdown: Arc<AtomicBool>,
}

/// The ONE entry point for creating a VirtualTui streaming session.
/// Calls the factory to obtain a `ViewConnection`, creates channels,
/// and spawns the VirtualTui thread.
pub fn start_virtual_tui_session(
    factory: &(dyn Fn() -> Option<ViewConnection> + Send + Sync),
    mouse: bool,
) -> Option<VirtualTuiSession> {
    let conn = factory()?;
    log::trace!(
        "[BIDI_TRACE] start_virtual_tui_session: ViewConnection obtained, starting VirtualTui"
    );
    let (output_tx, output_rx) = mpsc::channel(64);
    let (input_tx, input_rx) = mpsc::channel(64);
    let shutdown = Arc::new(AtomicBool::new(false));
    run_virtual_tui(conn, output_tx, input_rx, shutdown.clone(), mouse);
    Some(VirtualTuiSession {
        input_tx,
        output_rx,
        shutdown,
    })
}

/// What a VirtualTui prefixes a full repaint with (after a clear it is the first thing it writes).
const FULL_REPAINT_PREFIX: &[u8] = b"\x1b[2J\x1b[H";

/// Reset to initial state: a snapshot begins with it, so it can be written into a live terminal.
const RIS: &[u8] = b"\x1bc";

/// Stamps one connection's output with contiguous offsets and flags its full repaints.
///
/// A VirtualTui sends a repaint as its first frame and again whenever its buffers were cleared
/// (a [`VIRTUAL_TUI_REPAINT_REQUEST`], or a resize). Each is led by a clear, which is how it is
/// told apart from a delta; it is sent with a leading RIS and flagged as a snapshot, so the
/// client resets its screen model before painting it and re-anchors its continuity guard.
struct OutputStamper {
    next_offset: u64,
    opened: bool,
}

impl OutputStamper {
    fn new() -> Self {
        Self {
            next_offset: 0,
            opened: false,
        }
    }

    fn frame(&mut self, bytes: Vec<u8>) -> TerminalOutput {
        let snapshot = !self.opened || bytes.starts_with(FULL_REPAINT_PREFIX);
        self.opened = true;
        let data = if snapshot {
            let mut data = Vec::with_capacity(RIS.len() + bytes.len());
            data.extend_from_slice(RIS);
            data.extend_from_slice(&bytes);
            data
        } else {
            bytes
        };
        let start_offset = self.next_offset;
        self.next_offset += data.len() as u64;
        TerminalOutput {
            data,
            snapshot,
            start_offset,
            end_offset: self.next_offset,
        }
    }
}

/// What one input message asks of the VirtualTui: bytes to forward, and whether to repaint.
///
/// The connection's opening message is answered by the opening repaint, so its `request_snapshot`
/// asks for nothing more; any later one is applied after the message's own data.
fn forwarded_input(req: TerminalInput, opening: bool) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    if !req.data.is_empty() {
        chunks.push(req.data);
    }
    if req.request_snapshot && !opening {
        chunks.push(VIRTUAL_TUI_REPAINT_REQUEST.to_vec());
    }
    chunks
}

/// Per-connection VirtualTui terminal service.
/// Each `stream_terminal_io` call creates its own VirtualTui instance.
pub struct TerminalServiceVirtualTui {
    factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
    mouse: bool,
    /// Optional hook for each outbound terminal chunk (e.g. Codex OAuth scan).
    output_hook: Option<OutputChunkHook>,
}

impl TerminalServiceVirtualTui {
    pub fn new(
        factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
        mouse: bool,
    ) -> Self {
        Self {
            factory,
            mouse,
            output_hook: None,
        }
    }

    pub fn with_output_hook(
        factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
        mouse: bool,
        hook: OutputChunkHook,
    ) -> Self {
        Self {
            factory,
            mouse,
            output_hook: Some(hook),
        }
    }
}

#[async_trait]
impl TerminalService for TerminalServiceVirtualTui {
    type StreamTerminalIoStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIoStream>, Status> {
        let session = start_virtual_tui_session(&*self.factory, self.mouse)
            .ok_or_else(|| Status::internal("connect_view not available"))?;

        let VirtualTuiSession {
            input_tx,
            mut output_rx,
            shutdown,
        } = session;

        let client_stream = request.into_inner();
        tokio::spawn(async move {
            log::trace!("[BIDI_TRACE] terminal_service: input-forwarding task started");
            let mut input_count: u64 = 0;
            let mut cumulative_forwarded_bytes: u64 = 0;
            futures_util::pin_mut!(client_stream);
            while let Some(item) = futures_util::stream::StreamExt::next(&mut client_stream).await {
                input_count += 1;
                match item {
                    Ok(req) => {
                        for chunk in forwarded_input(req, input_count == 1) {
                            let chunk_len = chunk.len() as u64;
                            cumulative_forwarded_bytes += chunk_len;
                            if input_count <= 5 || input_count.is_multiple_of(500) || chunk_len != 1
                            {
                                log::debug!(
                                    "[BIDI_TRACE] terminal_service: forward msg#{} chunk_len={} cumulative_forwarded_bytes={}",
                                    input_count,
                                    chunk_len,
                                    cumulative_forwarded_bytes
                                );
                            }
                            if let Err(e) = input_tx.send(chunk).await {
                                log::error!(
                                    "[BIDI_TRACE] terminal_service: input_tx.send FAILED #{}: {}",
                                    input_count,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[BIDI_TRACE] terminal_service: stream error on input #{}: {}",
                            input_count,
                            e
                        );
                    }
                }
            }
            log::debug!(
                "[BIDI_TRACE] terminal_service: input stream ended after {} msgs cumulative_forwarded_bytes={}",
                input_count,
                cumulative_forwarded_bytes
            );
            shutdown.store(true, Ordering::Relaxed);
        });

        let (tx, rx) = mpsc::channel(64);
        let hook = self.output_hook.clone();
        tokio::spawn(async move {
            let mut output_count: u64 = 0;
            let mut stamper = OutputStamper::new();
            while let Some(bytes) = output_rx.recv().await {
                output_count += 1;
                if let Some(ref h) = hook {
                    h(&bytes);
                }
                if tx.send(Ok(stamper.frame(bytes))).await.is_err() {
                    log::debug!(
                        "[BIDI_TRACE] terminal_service: output consumer gone after {} chunks",
                        output_count,
                    );
                    break;
                }
            }
            log::debug!(
                "[BIDI_TRACE] terminal_service: output stream ended after {} chunks",
                output_count,
            );
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Tonic gRPC implementation for `TerminalServiceVirtualTui`.
/// Delegates to `start_virtual_tui_session` — same path as the LiveKit impl.
#[tonic::async_trait]
impl crate::tonic_terminal::terminal_service_server::TerminalService for TerminalServiceVirtualTui {
    type StreamTerminalIOStream = ReceiverStream<Result<TerminalOutput, tonic::Status>>;

    async fn stream_terminal_io(
        &self,
        request: tonic::Request<tonic::Streaming<TerminalInput>>,
    ) -> Result<tonic::Response<Self::StreamTerminalIOStream>, tonic::Status> {
        let session = start_virtual_tui_session(&*self.factory, self.mouse)
            .ok_or_else(|| tonic::Status::internal("connect_view not available"))?;

        let VirtualTuiSession {
            input_tx,
            mut output_rx,
            shutdown,
        } = session;

        let mut client_stream = request.into_inner();
        tokio::spawn(async move {
            log::trace!("tonic terminal_service: input-forwarding task started");
            let mut input_count: u64 = 0;
            let mut cumulative_forwarded_bytes: u64 = 0;
            while let Ok(Some(msg)) = client_stream.message().await {
                input_count += 1;
                for chunk in forwarded_input(msg, input_count == 1) {
                    let chunk_len = chunk.len() as u64;
                    cumulative_forwarded_bytes += chunk_len;
                    log::trace!(
                        "tonic: forwarding input #{} ({} bytes)",
                        input_count,
                        chunk_len
                    );
                    if input_count <= 5 || input_count.is_multiple_of(500) || chunk_len != 1 {
                        log::debug!(
                            "tonic VirtualTui input: msg#{} chunk_len={} cumulative_forwarded_bytes={}",
                            input_count,
                            chunk_len,
                            cumulative_forwarded_bytes
                        );
                    }
                    if let Err(e) = input_tx.send(chunk).await {
                        log::error!("tonic: input_tx.send FAILED #{}: {}", input_count, e);
                    }
                }
            }
            log::debug!(
                "tonic VirtualTui input stream ended: total_msgs={} cumulative_forwarded_bytes={}",
                input_count,
                cumulative_forwarded_bytes
            );
            shutdown.store(true, Ordering::Relaxed);
        });

        let hook = self.output_hook.clone();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            let mut stamper = OutputStamper::new();
            while let Some(bytes) = output_rx.recv().await {
                if let Some(ref h) = hook {
                    h(&bytes);
                }
                if tx.send(Ok(stamper.frame(bytes))).await.is_err() {
                    break;
                }
            }
            log::trace!("tonic output: stream ended");
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}
