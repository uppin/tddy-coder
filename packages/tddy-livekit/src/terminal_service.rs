//! Terminal service implementation for streaming TUI bytes over LiveKit.

use async_trait::async_trait;
use prost::Message;
use tokio::sync::{broadcast, mpsc};

use crate::bridge::{RpcResult, RpcService};
use crate::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};
use crate::status::Status;

/// Terminal service implementation.
/// Streams terminal output from a broadcast channel and forwards input to a sink.
pub struct TerminalServiceImpl {
    output_tx: broadcast::Sender<Vec<u8>>,
    input_tx: mpsc::Sender<Vec<u8>>,
}

impl TerminalServiceImpl {
    /// Create a new TerminalServiceImpl.
    /// - `output_tx`: broadcast sender for terminal output bytes (ANSI from TUI)
    /// - `input_tx`: sink for terminal input bytes (keyboard/mouse from client)
    pub fn new(output_tx: broadcast::Sender<Vec<u8>>, input_tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            output_tx,
            input_tx,
        }
    }
}

#[async_trait]
impl TerminalService for TerminalServiceImpl {
    async fn stream_terminal_io(
        &self,
        requests: Vec<TerminalInput>,
    ) -> Result<mpsc::Receiver<Result<TerminalOutput, Status>>, Status> {
        for req in &requests {
            if !req.data.is_empty() {
                let _ = self.input_tx.send(req.data.clone()).await;
            }
        }

        let mut output_rx = self.output_tx.subscribe();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            while let Ok(bytes) = output_rx.recv().await {
                let _ = tx.send(Ok(TerminalOutput { data: bytes })).await;
            }
        });

        Ok(rx)
    }
}

#[async_trait]
impl RpcService for TerminalServiceImpl {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "terminal.TerminalService"
            && (method == "StreamTerminalIO" || method == "streamTerminalIO")
    }

    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        request: &crate::proto::RpcRequest,
    ) -> RpcResult {
        if service == "terminal.TerminalService"
            && (method == "StreamTerminalIO" || method == "streamTerminalIO")
        {
            self.handle_rpc_stream(service, method, std::slice::from_ref(request))
                .await
        } else {
            RpcResult::Unary(Err(Status::unimplemented(
                "TerminalService uses handle_rpc_stream only",
            )))
        }
    }

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[crate::proto::RpcRequest],
    ) -> RpcResult {
        if service != "terminal.TerminalService" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown service: {}",
                service
            ))));
        }

        if method != "StreamTerminalIO" && method != "streamTerminalIO" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown method: {}",
                method
            ))));
        }

        let mut requests = Vec::with_capacity(messages.len());
        for msg in messages {
            if msg.request_message.is_empty() {
                continue;
            }
            match TerminalInput::decode(&msg.request_message[..]) {
                Ok(r) => requests.push(r),
                Err(e) => {
                    return RpcResult::ServerStream(Err(Status::invalid_argument(e.to_string())))
                }
            }
        }

        match self.stream_terminal_io(requests).await {
            Ok(rx) => {
                let (tx, new_rx) = mpsc::channel(16);
                tokio::spawn(async move {
                    let mut rx = rx;
                    while let Some(item) = rx.recv().await {
                        let bytes = item.map(|r| r.encode_to_vec());
                        let _ = tx.send(bytes).await;
                    }
                });
                RpcResult::ServerStream(Ok(new_rx))
            }
            Err(e) => RpcResult::ServerStream(Err(e)),
        }
    }
}
