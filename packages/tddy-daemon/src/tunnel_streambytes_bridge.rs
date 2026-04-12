//! LiveKit `LoopbackTunnelService.StreamBytes` bridging for operator-side TCP accept loops.
//!
//! Shared by [`crate::oauth_loopback_tunnel`] so byte framing stays consistent when
//! [`tddy_service::implementation_contract::LOOPBACK_TUNNEL_STREAMBYTES_VIA_GENERALIZED_SUPERVISOR`]
//! documents the extraction point for the managed tunnel product.

const _: () = assert!(
    tddy_service::implementation_contract::LOOPBACK_TUNNEL_STREAMBYTES_VIA_GENERALIZED_SUPERVISOR
);

use livekit::prelude::{ParticipantIdentity, Room};
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use tddy_livekit::RpcClient;
use tddy_service::proto::loopback_tunnel::TunnelChunk;

const LOG: &str = "tddy_daemon::tunnel_streambytes_bridge";

/// Bridges one accepted TCP connection to a bidi `StreamBytes` tunnel toward `target_identity`.
pub async fn bridge_tcp_to_tunnel(
    stream: TcpStream,
    room: std::sync::Arc<Room>,
    target_identity: ParticipantIdentity,
    remote_loopback_port: u16,
) {
    let events = room.subscribe();
    let client = RpcClient::new_shared(room, target_identity.clone(), events);
    let (mut sender, mut rx) =
        match client.start_bidi_stream("loopback_tunnel.LoopbackTunnelService", "StreamBytes") {
            Ok(x) => x,
            Err(e) => {
                log::error!(target: LOG, "start_bidi_stream failed: {}", e);
                return;
            }
        };

    let open = TunnelChunk {
        open_port: u32::from(remote_loopback_port),
        data: Vec::new(),
    };
    if let Err(e) = sender.send(open.encode_to_vec(), false).await {
        log::error!(target: LOG, "tunnel open chunk: {}", e);
        return;
    }

    let (mut rd, mut wr) = tokio::io::split(stream);
    let mut buf = vec![0u8; 65536];

    loop {
        tokio::select! {
            biased;
            n = rd.read(&mut buf) => {
                match n {
                    Ok(0) => {
                        let end = TunnelChunk {
                            open_port: 0,
                            data: Vec::new(),
                        };
                        let _ = sender.send(end.encode_to_vec(), true).await;
                        break;
                    }
                    Ok(n) => {
                        let chunk = TunnelChunk {
                            open_port: 0,
                            data: buf[..n].to_vec(),
                        };
                        if let Err(e) = sender.send(chunk.encode_to_vec(), false).await {
                            log::debug!(target: LOG, "tunnel upstream send: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        log::debug!(target: LOG, "tcp read: {}", e);
                        let end = TunnelChunk { open_port: 0, data: Vec::new() };
                        let _ = sender.send(end.encode_to_vec(), true).await;
                        break;
                    }
                }
            }
            msg = rx.recv() => {
                match msg {
                    Some(Ok(bytes)) => {
                        let chunk = match TunnelChunk::decode(&bytes[..]) {
                            Ok(c) => c,
                            Err(e) => {
                                log::debug!(target: LOG, "decode TunnelChunk: {}", e);
                                break;
                            }
                        };
                        if !chunk.data.is_empty() && wr.write_all(&chunk.data).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        log::debug!(target: LOG, "rpc stream error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
        }
    }
}

/// Binds `127.0.0.1:listen_port` and spawns [`bridge_tcp_to_tunnel`] per accepted connection.
pub async fn run_tcp_accept_loop(
    room: std::sync::Arc<Room>,
    target_identity: ParticipantIdentity,
    listen_port: u16,
    remote_loopback_port: u16,
) -> std::io::Result<()> {
    let addr = (std::net::Ipv4Addr::LOCALHOST, listen_port);
    let listener = TcpListener::bind(addr).await?;
    log::info!(
        target: LOG,
        "loopback TCP listening on 127.0.0.1:{} (StreamBytes → {:?} @ 127.0.0.1:{})",
        listen_port,
        target_identity,
        remote_loopback_port
    );
    loop {
        let (stream, _) = listener.accept().await?;
        let room = room.clone();
        let tid = target_identity.clone();
        let rp = remote_loopback_port;
        tokio::spawn(async move {
            bridge_tcp_to_tunnel(stream, room, tid, rp).await;
        });
    }
}
