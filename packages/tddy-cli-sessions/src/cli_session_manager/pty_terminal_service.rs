use async_trait::async_trait;
use prost::Message as _;

use tddy_rpc::ResponseBody;

use tddy_service::proto::terminal::TerminalInput;

use tddy_service::proto::terminal::TerminalOutput;

use tddy_rpc::BidiStreamOutput;

use tddy_rpc::RpcResult;

use tddy_rpc::RpcMessage;

use tddy_rpc::RpcService;
use tddy_task::ScreenSnapshot;
use tokio::sync::{broadcast, mpsc, Notify};

use std::sync::Arc;

use crate::cli_session_manager::pty_handle;

/// RPC service that bridges `terminal.TerminalService/StreamTerminalIO` to a PTY handle.
struct PtyTerminalService {
    pub(crate) handle: Arc<pty_handle::PtyHandle>,
}

#[async_trait]
impl RpcService for PtyTerminalService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "terminal.TerminalService" && method == "StreamTerminalIO"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, _msg: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(tddy_rpc::Status::unimplemented("use bidi stream")))
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        _metadata: tddy_rpc::RequestMetadata,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, tddy_rpc::Status> {
        if service != "terminal.TerminalService" || method != "StreamTerminalIO" {
            return Err(tddy_rpc::Status::not_found(format!(
                "{}/{}",
                service, method
            )));
        }

        let (out_tx, out_rx) = mpsc::channel::<Result<Vec<u8>, tddy_rpc::Status>>(256);

        // Open with a snapshot of the screen as it is now. There is no raw replay and no SIGWINCH:
        // the snapshot carries the modes the application enabled, so the client's own VT ends up
        // where the application left it, and the application is never nudged to repaint.
        let (snapshot, live) = snapshot_and_subscribe(&self.handle)?;
        let _ = out_tx.try_send(Ok(snapshot_frame(snapshot.data, snapshot.offset)));
        let mut live = LiveOutput {
            rx: live,
            next_offset: snapshot.offset,
        };

        // A mid-stream `request_snapshot` makes the output half re-take the snapshot.
        let resync = Arc::new(Notify::new());

        // PTY stdout → bidi output stream. Breaks when:
        // - broadcast sender closed (session dropped)
        // - pty_done watch fires (reader exited; no more output coming)
        // - the client disconnected, or the screen could not be snapshotted
        let handle_for_output = Arc::clone(&self.handle);
        let mut pty_done = self.handle.pty_done.clone();
        let resync_for_output = Arc::clone(&resync);
        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = live.rx.recv() => {
                        match result {
                            Ok(bytes) => {
                                let frame = live.data_frame(&bytes);
                                if out_tx_clone.send(Ok(frame)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "terminal exposure: PTY output lagged {} messages; resyncing with a snapshot",
                                    n
                                );
                                if !live.resync(&handle_for_output, &out_tx_clone).await {
                                    break;
                                }
                            }
                        }
                    }
                    _ = resync_for_output.notified() => {
                        if !live.resync(&handle_for_output, &out_tx_clone).await {
                            break;
                        }
                    }
                    // PTY reader exited — drain any remaining broadcast items then stop.
                    _ = pty_done.changed() => break,
                }
            }
        });

        // Bidi input stream → PTY stdin. Resize escape sequences are intercepted and applied
        // to the PTY via SIGWINCH rather than forwarded as raw bytes.
        let handle_for_input = Arc::clone(&self.handle);
        tokio::spawn(async move {
            let mut opening = true;
            while let Some(msg) = input_rx.recv().await {
                if let Ok(input) = TerminalInput::decode(&msg.payload[..]) {
                    if !input.data.is_empty() {
                        // VirtualTui `TerminalInput` carries no offset; 0 = unset (no ACK).
                        handle_for_input.send_input(bytes::Bytes::from(input.data), 0);
                    }
                    // The open already sent a snapshot, which answers the opening message's ask.
                    // Any other ask is applied after the message's data.
                    if input.request_snapshot && !opening {
                        resync.notify_one();
                    }
                }
                opening = false;
            }
        });

        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(out_rx),
        })
    }
}

/// The RPC surface driving one PTY — `terminal.TerminalService` — built from `tddy-rpc` types only,
/// for whatever exposes it.
pub fn pty_terminal_service(handle: Arc<pty_handle::PtyHandle>) -> Arc<dyn tddy_rpc::RpcService> {
    Arc::new(PtyTerminalService { handle })
}

/// The screen and a live-output receiver that starts exactly where it ends.
///
/// Both are taken under the capture lock, which every producer holds while it appends and
/// broadcasts, so the receiver yields precisely the bytes from `snapshot.offset` on.
fn snapshot_and_subscribe(
    handle: &pty_handle::PtyHandle,
) -> Result<(ScreenSnapshot, broadcast::Receiver<bytes::Bytes>), tddy_rpc::Status> {
    let capture = handle
        .capture
        .lock()
        .map_err(|_| tddy_rpc::Status::internal("the terminal capture is poisoned"))?;
    let snapshot = capture.snapshot().ok_or_else(|| {
        tddy_rpc::Status::failed_precondition(
            "the terminal's size is unknown, so its screen cannot be snapshotted",
        )
    })?;
    let live = handle.stdout_tx.subscribe();
    drop(capture);
    Ok((snapshot, live))
}

/// A snapshot frame: one repaint spanning `[0, end_offset)`.
fn snapshot_frame(data: Vec<u8>, end_offset: u64) -> Vec<u8> {
    TerminalOutput {
        data,
        snapshot: true,
        start_offset: 0,
        end_offset,
    }
    .encode_to_vec()
}

/// The live half of an output stream: the receiver and the absolute offset the next byte sits at.
struct LiveOutput {
    rx: broadcast::Receiver<bytes::Bytes>,
    next_offset: u64,
}

impl LiveOutput {
    /// A live data frame, stamped with the contiguous offsets it occupies.
    fn data_frame(&mut self, bytes: &bytes::Bytes) -> Vec<u8> {
        let start_offset = self.next_offset;
        let end_offset = start_offset + bytes.len() as u64;
        self.next_offset = end_offset;
        TerminalOutput {
            data: bytes.to_vec(),
            snapshot: false,
            start_offset,
            end_offset,
        }
        .encode_to_vec()
    }

    /// Re-take the snapshot and send it: the receiver is replaced (whatever the old one had queued
    /// is covered by the snapshot) and stamping restarts from the snapshot's offset. `false` when
    /// the stream must stop: the client is gone, or the screen could not be snapshotted.
    async fn resync(
        &mut self,
        handle: &pty_handle::PtyHandle,
        out_tx: &mpsc::Sender<Result<Vec<u8>, tddy_rpc::Status>>,
    ) -> bool {
        match snapshot_and_subscribe(handle) {
            Ok((snapshot, live)) => {
                self.rx = live;
                self.next_offset = snapshot.offset;
                out_tx
                    .send(Ok(snapshot_frame(snapshot.data, snapshot.offset)))
                    .await
                    .is_ok()
            }
            Err(status) => {
                let _ = out_tx.send(Err(status)).await;
                false
            }
        }
    }
}
