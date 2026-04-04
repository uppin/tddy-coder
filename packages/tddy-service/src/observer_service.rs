//! PresenterObserver gRPC: passive server-stream of Presenter events (no client intents).

use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use tddy_core::PresenterEvent;

use crate::convert::event_to_server_message;
use crate::gen::presenter_observer_server::PresenterObserver;
use crate::gen::{ObserveRequest, ServerMessage};

/// Subscribes to the Presenter broadcast and streams [`ServerMessage`] events to gRPC clients.
pub struct PresenterObserverService {
    event_tx: broadcast::Sender<PresenterEvent>,
}

impl PresenterObserverService {
    pub fn new(event_tx: broadcast::Sender<PresenterEvent>) -> Self {
        Self { event_tx }
    }
}

#[tonic::async_trait]
impl PresenterObserver for PresenterObserverService {
    type ObserveEventsStream = ReceiverStream<Result<ServerMessage, Status>>;

    async fn observe_events(
        &self,
        _request: Request<ObserveRequest>,
    ) -> Result<Response<Self::ObserveEventsStream>, Status> {
        let mut event_rx = self.event_tx.subscribe();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        let msg = event_to_server_message(event);
                        if tx.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "PresenterObserver gRPC stream: broadcast receiver lagged; skipped {} presenter event(s)",
                            skipped
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
