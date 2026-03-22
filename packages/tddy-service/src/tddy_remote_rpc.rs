//! [`tddy_rpc::RpcService`] adapter for [`crate::gen::tddy_remote_server::TddyRemote`] / `Stream`
//! (used by LiveKit and other non-tonic transports).

use async_trait::async_trait;
use prost::Message;
use tokio::sync::{broadcast, mpsc};

use tddy_core::PresenterHandle;
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage, RpcService, Status};

use crate::convert::{client_message_to_intent, event_to_server_message};
use crate::gen::{tddy_remote_server, ClientMessage};

/// LiveKit / tddy-rpc server for `tddy.v1.TddyRemote` bidirectional `Stream`.
pub struct TddyRemoteRpcServer {
    event_tx: broadcast::Sender<tddy_core::PresenterEvent>,
    intent_tx: std::sync::mpsc::Sender<tddy_core::UserIntent>,
}

impl TddyRemoteRpcServer {
    pub const NAME: &'static str = tddy_remote_server::SERVICE_NAME;

    pub fn new(handle: PresenterHandle) -> Self {
        Self {
            event_tx: handle.event_tx,
            intent_tx: handle.intent_tx,
        }
    }
}

#[async_trait]
impl RpcService for TddyRemoteRpcServer {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == Self::NAME && method == "Stream"
    }

    async fn handle_rpc(
        &self,
        _service: &str,
        _method: &str,
        _message: &RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(Status::unimplemented(
            "TddyRemote unary not used; use Stream bidi",
        )))
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        if service != Self::NAME || method != "Stream" {
            return Err(Status::not_found(format!(
                "unknown TddyRemote method {}/{}",
                service, method
            )));
        }

        log::info!(
            "TddyRemoteRpcServer: start_bidi_stream {}/{}",
            service,
            method
        );

        let mut event_rx = self.event_tx.subscribe();
        let intent_tx = self.intent_tx.clone();

        let (out_tx, out_rx) = mpsc::channel::<Result<Vec<u8>, Status>>(64);

        tokio::spawn(async move {
            while let Some(msg) = input_rx.recv().await {
                if msg.payload.is_empty() {
                    continue;
                }
                match ClientMessage::decode(&msg.payload[..]) {
                    Ok(cm) => {
                        if let Some(intent) = client_message_to_intent(cm) {
                            log::debug!("TddyRemoteRpcServer: intent from client {:?}", intent);
                            if intent_tx.send(intent).is_err() {
                                log::warn!("TddyRemoteRpcServer: intent_tx closed");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("TddyRemoteRpcServer: decode ClientMessage: {}", e);
                    }
                }
            }
        });

        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        let msg = event_to_server_message(event);
                        if msg.event.is_none() {
                            continue;
                        }
                        log::debug!("TddyRemoteRpcServer: outbound ServerMessage event present");
                        if out_tx.send(Ok(msg.encode_to_vec())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::debug!("TddyRemoteRpcServer: event_rx lagged skipped={}", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(out_rx),
        })
    }
}
