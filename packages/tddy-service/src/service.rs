//! TddyRemote gRPC service implementation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use tokio::sync::{broadcast, mpsc as tokio_mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::codec::Streaming;
use tonic::{Request, Response, Status};

use tddy_core::{PresenterEvent, PresenterHandle, ViewConnection};
use tddy_tui::run_virtual_tui;

use crate::convert::{client_message_to_intent, event_to_server_message};
use crate::gen::{
    tddy_remote_server::TddyRemote, ClientMessage, GetSessionRequest, GetSessionResponse,
    ListSessionsRequest, ListSessionsResponse, ServerMessage, StreamTerminalRequest,
    TerminalOutput,
};

/// gRPC service that bridges Presenter events and intents.
/// Implements the Stream RPC (renamed from Connect to avoid conflict with tonic client).
pub struct TddyRemoteService {
    event_tx: broadcast::Sender<PresenterEvent>,
    intent_tx: mpsc::Sender<tddy_core::UserIntent>,
    terminal_byte_tx: Option<broadcast::Sender<Vec<u8>>>,
    view_connection_factory: Option<Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>>,
}

impl TddyRemoteService {
    pub fn new(handle: PresenterHandle) -> Self {
        Self {
            event_tx: handle.event_tx,
            intent_tx: handle.intent_tx,
            terminal_byte_tx: None,
            view_connection_factory: None,
        }
    }

    pub fn with_terminal_bytes(mut self, tx: broadcast::Sender<Vec<u8>>) -> Self {
        self.terminal_byte_tx = Some(tx);
        self
    }

    /// Use per-connection VirtualTui instead of shared broadcast for stream_terminal_io.
    pub fn with_view_connection_factory(
        mut self,
        factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
    ) -> Self {
        self.view_connection_factory = Some(factory);
        self
    }
}

#[tonic::async_trait]
impl TddyRemote for TddyRemoteService {
    type StreamStream = ReceiverStream<Result<ServerMessage, Status>>;
    type StreamTerminalStream = ReceiverStream<Result<TerminalOutput, Status>>;
    type StreamTerminalIOStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream(
        &self,
        request: Request<Streaming<ClientMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let mut event_rx = self.event_tx.subscribe();
        let intent_tx = self.intent_tx.clone();
        let mut client_stream = request.into_inner();

        let (tx, rx) = tokio::sync::mpsc::channel(64);

        // Spawn task: receive from broadcast, convert, send to response stream
        let event_tx_clone = tx.clone();
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        let msg = event_to_server_message(event);
                        if event_tx_clone.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_n)) => {
                        // Lagged receiver - client could not keep up
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Spawn task: receive from client stream, convert to intent, send to presenter
        tokio::spawn(async move {
            while let Ok(Some(msg)) = client_stream.message().await {
                if let Some(intent) = client_message_to_intent(msg) {
                    let _ = intent_tx.send(intent);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn stream_terminal(
        &self,
        _request: Request<StreamTerminalRequest>,
    ) -> Result<Response<Self::StreamTerminalStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        if let Some(ref byte_tx) = self.terminal_byte_tx {
            let mut byte_rx = byte_tx.subscribe();
            tokio::spawn(async move {
                loop {
                    match byte_rx.recv().await {
                        Ok(data) => {
                            if tx.send(Ok(TerminalOutput { data })).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        } else {
            drop(tx);
        }

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<crate::gen::TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIOStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let mut client_stream = request.into_inner();

        if let Some(ref factory) = self.view_connection_factory {
            if let Some(conn) = factory() {
                let (output_tx, mut output_rx) = tokio_mpsc::channel(64);
                let (input_tx, input_rx) = tokio_mpsc::channel(64);
                let shutdown = Arc::new(AtomicBool::new(false));
                let shutdown_clone = shutdown.clone();

                run_virtual_tui(conn, output_tx, input_rx, shutdown_clone);

                tokio::spawn(async move {
                    while let Ok(Some(msg)) = client_stream.message().await {
                        if !msg.data.is_empty() {
                            let _ = input_tx.send(msg.data).await;
                        }
                    }
                    shutdown.store(true, Ordering::Relaxed);
                });

                tokio::spawn(async move {
                    while let Some(bytes) = output_rx.recv().await {
                        if tx.send(Ok(TerminalOutput { data: bytes })).await.is_err() {
                            break;
                        }
                    }
                });
            }
        } else if let Some(ref byte_tx) = self.terminal_byte_tx {
            let mut byte_rx = byte_tx.subscribe();
            tokio::spawn(async move {
                loop {
                    match byte_rx.recv().await {
                        Ok(data) => {
                            if tx.send(Ok(TerminalOutput { data })).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            tokio::spawn(async move {
                while let Ok(Some(_input)) = client_stream.message().await {
                    // Terminal input could be forwarded to PTY; for now just consume
                }
            });
        } else {
            drop(tx);
        }

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_session(
        &self,
        _request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        Err(Status::unimplemented(
            "GetSession is only available in daemon mode",
        ))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        Err(Status::unimplemented(
            "ListSessions is only available in daemon mode",
        ))
    }
}
