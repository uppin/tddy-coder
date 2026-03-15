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
use tddy_tui::run_virtual_tui;

use crate::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};

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
) -> Option<VirtualTuiSession> {
    let conn = factory()?;
    log::trace!(
        "[BIDI_TRACE] start_virtual_tui_session: ViewConnection obtained, starting VirtualTui"
    );
    let (output_tx, output_rx) = mpsc::channel(64);
    let (input_tx, input_rx) = mpsc::channel(64);
    let shutdown = Arc::new(AtomicBool::new(false));
    run_virtual_tui(conn, output_tx, input_rx, shutdown.clone());
    Some(VirtualTuiSession {
        input_tx,
        output_rx,
        shutdown,
    })
}

/// Per-connection VirtualTui terminal service.
/// Each `stream_terminal_io` call creates its own VirtualTui instance.
pub struct TerminalServiceVirtualTui {
    factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
}

impl TerminalServiceVirtualTui {
    pub fn new(factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>) -> Self {
        Self { factory }
    }
}

#[async_trait]
impl TerminalService for TerminalServiceVirtualTui {
    type StreamTerminalIoStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIoStream>, Status> {
        let session = start_virtual_tui_session(&*self.factory)
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
            futures_util::pin_mut!(client_stream);
            while let Some(item) = futures_util::stream::StreamExt::next(&mut client_stream).await {
                input_count += 1;
                match item {
                    Ok(req) if !req.data.is_empty() => {
                        if let Err(e) = input_tx.send(req.data).await {
                            log::error!(
                                "[BIDI_TRACE] terminal_service: input_tx.send FAILED #{}: {}",
                                input_count,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[BIDI_TRACE] terminal_service: stream error on input #{}: {}",
                            input_count,
                            e
                        );
                    }
                    _ => {}
                }
            }
            log::trace!(
                "[BIDI_TRACE] terminal_service: input stream ended after {} inputs",
                input_count
            );
            shutdown.store(true, Ordering::Relaxed);
        });

        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(bytes) = output_rx.recv().await {
                if tx.send(Ok(TerminalOutput { data: bytes })).await.is_err() {
                    break;
                }
            }
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
        let session = start_virtual_tui_session(&*self.factory)
            .ok_or_else(|| tonic::Status::internal("connect_view not available"))?;

        let VirtualTuiSession {
            input_tx,
            mut output_rx,
            shutdown,
        } = session;

        let mut client_stream = request.into_inner();
        tokio::spawn(async move {
            log::trace!("[BIDI_TRACE] tonic terminal_service: input-forwarding task started");
            let mut input_count: u64 = 0;
            while let Ok(Some(msg)) = client_stream.message().await {
                input_count += 1;
                if !msg.data.is_empty() {
                    eprintln!(
                        "[BIDI_TRACE] tonic: forwarding input #{} ({} bytes): {:?}",
                        input_count,
                        msg.data.len(),
                        &msg.data
                    );
                    if let Err(e) = input_tx.send(msg.data).await {
                        eprintln!(
                            "[BIDI_TRACE] tonic: input_tx.send FAILED #{}: {}",
                            input_count, e
                        );
                    }
                } else {
                    eprintln!(
                        "[BIDI_TRACE] tonic: input #{} is empty (init), skipping",
                        input_count
                    );
                }
            }
            eprintln!(
                "[BIDI_TRACE] tonic: input stream ended after {} inputs",
                input_count
            );
            shutdown.store(true, Ordering::Relaxed);
        });

        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            let mut fwd_count: u64 = 0;
            while let Some(bytes) = output_rx.recv().await {
                fwd_count += 1;
                if fwd_count <= 5 || fwd_count % 100 == 0 {
                    eprintln!("[BIDI_TRACE] tonic output: fwd#{} ({} bytes)", fwd_count, bytes.len());
                }
                if tx.send(Ok(TerminalOutput { data: bytes })).await.is_err() {
                    eprintln!("[BIDI_TRACE] tonic output: tx.send failed at fwd#{}", fwd_count);
                    break;
                }
            }
            eprintln!("[BIDI_TRACE] tonic output: stream ended after {} fwds", fwd_count);
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}
