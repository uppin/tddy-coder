use async_trait::async_trait;
use prost::Message as _;

use tddy_rpc::ResponseBody;

use tddy_service::proto::terminal::TerminalInput;

use tddy_service::proto::terminal::TerminalOutput;

use tddy_rpc::BidiStreamOutput;

use tddy_rpc::RpcResult;

use tddy_rpc::RpcMessage;

use tddy_rpc::RpcService;
use tokio::sync::{broadcast, mpsc};

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

        // Replay the mouse-tracking prologue plus all output since session start, so the client's
        // own VT ends up in the same modes the application enabled — even when the bytes that
        // enabled them have long been evicted from the capture ring.
        if let Ok(cap) = self.handle.capture.lock() {
            let replay = cap.replay();
            if !replay.is_empty() {
                let frame = TerminalOutput { data: replay }.encode_to_vec();
                let _ = out_tx.try_send(Ok(frame));
            }
        }

        // PTY stdout → bidi output stream. Breaks when:
        // - broadcast sender closed (session dropped)
        // - pty_done watch fires (reader exited; no more output coming)
        // - out_tx_clone send fails (client disconnected)
        let mut stdout_rx = self.handle.stdout_tx.subscribe();
        let mut pty_done = self.handle.pty_done.clone();
        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stdout_rx.recv() => {
                        match result {
                            Ok(bytes) => {
                                let frame = TerminalOutput { data: bytes.to_vec() }.encode_to_vec();
                                if out_tx_clone.send(Ok(frame)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "terminal exposure: PTY output lagged {} messages",
                                    n
                                );
                            }
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
            while let Some(msg) = input_rx.recv().await {
                if let Ok(input) = TerminalInput::decode(&msg.payload[..]) {
                    if !input.data.is_empty() {
                        // VirtualTui `TerminalInput` carries no offset; 0 = unset (no ACK).
                        handle_for_input.send_input(bytes::Bytes::from(input.data), 0);
                    }
                }
            }
        });

        // Trigger a SIGWINCH so the TUI repaints for the new subscriber.
        self.handle.trigger_redraw();

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
