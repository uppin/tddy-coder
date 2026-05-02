//! LiveKit bidi tunnel: raw TCP on the session host loopback, framed as `TunnelChunk` over RPC.

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use tddy_rpc::{Request, Response, Status, Streaming};

use crate::proto::loopback_tunnel::{LoopbackTunnelService, TunnelChunk};

/// Forwards each bidi session to `127.0.0.1:{open_port}` on the agent host.
pub struct LoopbackTunnelServiceImpl;

#[async_trait]
impl LoopbackTunnelService for LoopbackTunnelServiceImpl {
    type StreamBytesStream = ReceiverStream<Result<TunnelChunk, Status>>;

    async fn stream_bytes(
        &self,
        request: Request<Streaming<TunnelChunk>>,
    ) -> Result<Response<Self::StreamBytesStream>, Status> {
        let mut client_stream = request.into_inner();
        let first = futures_util::StreamExt::next(&mut client_stream)
            .await
            .ok_or_else(|| Status::invalid_argument("empty tunnel stream"))?
            .map_err(|_| Status::internal("tunnel stream error"))?;

        let port = first.open_port;
        if port == 0 || port > 65535 {
            return Err(Status::invalid_argument(
                "first TunnelChunk must set open_port (1..=65535)",
            ));
        }
        if port < 1024 {
            return Err(Status::invalid_argument(
                "open_port must be >= 1024 (refusing privileged loopback ports)",
            ));
        }

        let addr = format!("127.0.0.1:{}", port);
        let tcp = TcpStream::connect(&addr)
            .await
            .map_err(|e| Status::internal(format!("tunnel connect {}: {}", addr, e)))?;

        let (mut read_half, mut write_half) = tcp.into_split();
        if !first.data.is_empty() {
            write_half
                .write_all(&first.data)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        let (resp_tx, resp_rx) = mpsc::channel::<Result<TunnelChunk, Status>>(64);

        tokio::spawn(async move {
            while let Some(item) = futures_util::StreamExt::next(&mut client_stream).await {
                match item {
                    Ok(chunk) => {
                        if chunk.open_port != 0 {
                            log::warn!(
                                target: "tddy_service::loopback_tunnel",
                                "ignoring non-zero open_port on follow-up chunk"
                            );
                        }
                        if chunk.data.is_empty() {
                            continue;
                        }
                        if write_half.write_all(&chunk.data).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        log::debug!(
                            target: "tddy_service::loopback_tunnel",
                            "client chunk error: {}",
                            e
                        );
                        break;
                    }
                }
            }
            let _ = write_half.shutdown().await;
        });

        let resp_tx_read = resp_tx.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = TunnelChunk {
                            open_port: 0,
                            data: buf[..n].to_vec(),
                        };
                        if resp_tx_read.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        log::debug!(
                            target: "tddy_service::loopback_tunnel",
                            "tcp read error: {}",
                            e
                        );
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(resp_rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::Request;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn stream_bytes_forwards_ping_pong() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 32];
            let n = sock.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"ping");
            sock.write_all(b"pong").await.unwrap();
        });

        let (in_tx, in_rx) = mpsc::channel::<Result<TunnelChunk, Status>>(8);
        in_tx
            .send(Ok(TunnelChunk {
                open_port: port as u32,
                data: vec![],
            }))
            .await
            .unwrap();
        in_tx
            .send(Ok(TunnelChunk {
                open_port: 0,
                data: b"ping".to_vec(),
            }))
            .await
            .unwrap();
        drop(in_tx);

        let streaming = Streaming::new(ReceiverStream::new(in_rx));
        let svc = LoopbackTunnelServiceImpl;
        let resp = svc
            .stream_bytes(Request::new(streaming))
            .await
            .expect("stream_bytes");
        let out = resp.into_inner();
        futures_util::pin_mut!(out);
        let chunk = futures_util::StreamExt::next(&mut out)
            .await
            .expect("one chunk")
            .expect("ok");
        assert_eq!(chunk.data, b"pong");
    }
}
