//! TddyRemote gRPC service implementation.

use std::sync::mpsc;

use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codec::Streaming;
use tonic::{Request, Response, Status};

use tddy_core::{PresenterEvent, PresenterHandle};

use crate::convert::{client_message_to_intent, event_to_server_message};
use crate::gen::{
    tddy_remote_server::TddyRemote, ClientMessage, GetSessionRequest, GetSessionResponse,
    ListSessionsRequest, ListSessionsResponse, ServerMessage,
};

/// gRPC service that bridges Presenter events and intents.
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
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "TddyRemote gRPC stream: broadcast receiver lagged; skipped {} presenter event(s)",
                            skipped
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

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

/// Second transport for the same `TddyRemoteService`: an `RpcService`-compatible impl so it can
/// be registered in a LiveKit `MultiRpcService` (the plain-gRPC impl above only satisfies
/// tonic's server trait, which `MultiRpcService` cannot wrap). The message types on this side
/// come from a separate codegen pass (`crate::proto::tddy_remote`, see build.rs) and are
/// nominally distinct from `crate::gen`'s tonic-generated types even though both are compiled
/// from the same remote.proto — `rpc_*`/`tonic_*` helpers below bridge the two by re-encoding
/// and decoding, which is exact since both sides share the same wire format.
mod livekit_transport {
    use futures_util::StreamExt;
    use prost::Message;
    use tokio_stream::wrappers::ReceiverStream;

    use crate::convert::{client_message_to_intent, event_to_server_message};
    use crate::gen::{ClientMessage as TonicClientMessage, ServerMessage as TonicServerMessage};
    use crate::proto::tddy_remote::{
        ClientMessage, GetSessionRequest, GetSessionResponse, ListSessionsRequest,
        ListSessionsResponse, ServerMessage, TddyRemote,
    };

    use super::TddyRemoteService;

    fn to_tonic_client_message(msg: ClientMessage) -> TonicClientMessage {
        TonicClientMessage::decode(&msg.encode_to_vec()[..])
            .expect("ClientMessage is wire-compatible across both TddyRemote codegen passes")
    }

    fn from_tonic_server_message(msg: TonicServerMessage) -> ServerMessage {
        ServerMessage::decode(&msg.encode_to_vec()[..])
            .expect("ServerMessage is wire-compatible across both TddyRemote codegen passes")
    }

    #[async_trait::async_trait]
    impl TddyRemote for TddyRemoteService {
        type StreamStream = ReceiverStream<Result<ServerMessage, tddy_rpc::Status>>;

        async fn stream(
            &self,
            request: tddy_rpc::Request<tddy_rpc::Streaming<ClientMessage>>,
        ) -> Result<tddy_rpc::Response<Self::StreamStream>, tddy_rpc::Status> {
            let mut event_rx = self.event_tx.subscribe();
            let intent_tx = self.intent_tx.clone();
            let mut client_stream = request.into_inner();

            let (tx, rx) = tokio::sync::mpsc::channel(64);

            let event_tx_clone = tx.clone();
            tokio::spawn(async move {
                loop {
                    match event_rx.recv().await {
                        Ok(event) => {
                            let msg = from_tonic_server_message(event_to_server_message(event));
                            if event_tx_clone.send(Ok(msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            log::warn!(
                                "TddyRemote LiveKit stream: broadcast receiver lagged; skipped {} presenter event(s)",
                                skipped
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            tokio::spawn(async move {
                while let Some(item) = client_stream.next().await {
                    if let Ok(msg) = item {
                        if let Some(intent) = client_message_to_intent(to_tonic_client_message(msg))
                        {
                            let _ = intent_tx.send(intent);
                        }
                    }
                }
            });

            Ok(tddy_rpc::Response::new(ReceiverStream::new(rx)))
        }

        async fn get_session(
            &self,
            _request: tddy_rpc::Request<GetSessionRequest>,
        ) -> Result<tddy_rpc::Response<GetSessionResponse>, tddy_rpc::Status> {
            Err(tddy_rpc::Status::unimplemented(
                "GetSession is only available in daemon mode",
            ))
        }

        async fn list_sessions(
            &self,
            _request: tddy_rpc::Request<ListSessionsRequest>,
        ) -> Result<tddy_rpc::Response<ListSessionsResponse>, tddy_rpc::Status> {
            Err(tddy_rpc::Status::unimplemented(
                "ListSessions is only available in daemon mode",
            ))
        }
    }
}
