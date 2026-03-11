//! TddyRemote gRPC service implementation.

use std::sync::mpsc;

use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codec::Streaming;
use tonic::{Request, Response, Status};

use tddy_core::PresenterEvent;
use tddy_core::PresenterHandle;

use crate::convert::{client_message_to_intent, event_to_server_message};
use crate::gen::{
    tddy_remote_server::TddyRemote, ClientMessage, GetSessionRequest, GetSessionResponse,
    ListSessionsRequest, ListSessionsResponse, ServerMessage,
};

/// gRPC service that bridges Presenter events and intents.
/// Implements the Stream RPC (renamed from Connect to avoid conflict with tonic client).
pub struct TddyRemoteService {
    event_tx: broadcast::Sender<PresenterEvent>,
    intent_tx: mpsc::Sender<tddy_core::UserIntent>,
}

impl TddyRemoteService {
    pub fn new(handle: PresenterHandle) -> Self {
        Self {
            event_tx: handle.event_tx,
            intent_tx: handle.intent_tx,
        }
    }
}

#[tonic::async_trait]
impl TddyRemote for TddyRemoteService {
    type StreamStream = ReceiverStream<Result<ServerMessage, Status>>;

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
