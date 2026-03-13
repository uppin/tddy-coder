//! Terminal service implementation for streaming TUI bytes.

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use tddy_rpc::{Request, Response, Status, Streaming};

use crate::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};

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
    type StreamTerminalIoStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIoStream>, Status> {
        let stream = request.into_inner();
        let input_tx = self.input_tx.clone();

        tokio::spawn(async move {
            futures_util::pin_mut!(stream);
            while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
                if let Ok(req) = item {
                    if !req.data.is_empty() {
                        let _ = input_tx.send(req.data).await;
                    }
                }
            }
        });

        let mut output_rx = self.output_tx.subscribe();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            while let Ok(bytes) = output_rx.recv().await {
                let _ = tx.send(Ok(TerminalOutput { data: bytes })).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
