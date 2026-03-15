//! Echo service implementation for testing.

use async_trait::async_trait;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use tddy_rpc::{Request, Response, RpcBridge, Status, Streaming};

use crate::proto::test::{EchoRequest, EchoResponse, EchoService, EchoServiceServer};

/// Echo service implementation.
pub struct EchoServiceImpl;

#[async_trait]
impl EchoService for EchoServiceImpl {
    type EchoServerStreamStream = ReceiverStream<Result<EchoResponse, Status>>;
    type EchoBidiStreamStream = ReceiverStream<Result<EchoResponse, Status>>;

    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(Response::new(EchoResponse {
            message: req.message,
            timestamp,
        }))
    }

    async fn echo_server_stream(
        &self,
        request: Request<EchoRequest>,
    ) -> Result<Response<Self::EchoServerStreamStream>, Status> {
        let req = request.into_inner();
        let message = req.message;
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0u32..3 {
                let _ = tx
                    .send(Ok(EchoResponse {
                        message: format!("{} #{}", message, i + 1),
                        timestamp: 0,
                    }))
                    .await;
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn echo_client_stream(
        &self,
        request: Request<Streaming<EchoRequest>>,
    ) -> Result<Response<EchoResponse>, Status> {
        let stream = request.into_inner();
        let mut requests = Vec::new();
        futures_util::pin_mut!(stream);
        while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
            requests.push(item?);
        }
        let joined = requests
            .iter()
            .map(|r| r.message.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(Response::new(EchoResponse {
            message: joined,
            timestamp,
        }))
    }

    async fn echo_bidi_stream(
        &self,
        request: Request<Streaming<EchoRequest>>,
    ) -> Result<Response<Self::EchoBidiStreamStream>, Status> {
        let stream = request.into_inner();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            futures_util::pin_mut!(stream);
            let mut seq = 0u32;
            while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
                match item {
                    Ok(req) => {
                        seq += 1;
                        let _ = tx
                            .send(Ok(EchoResponse {
                                message: format!("{} #{}", req.message, seq),
                                timestamp: 0,
                            }))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                    }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Create an RpcBridge with EchoServiceImpl (wrapped in generated server).
pub fn create_echo_bridge() -> RpcBridge<EchoServiceServer<EchoServiceImpl>> {
    RpcBridge::new(EchoServiceServer::new(EchoServiceImpl))
}
