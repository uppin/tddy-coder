//! A configurable in-process `SandboxService` fake for host-relay and transport tests.
//!
// Each test binary uses a different subset of the fake's modes/helpers, so some appear unused
// per-binary.
#![allow(dead_code)]
//!
//! It plays the role of the in-jail runner: over `SessionChannel` it can push a `ToolRequest` or a
//! `TunnelOpen` to the connected host and records what the host sends back (`ToolResponse`,
//! `TunnelOpenAck`). `Echo` is a real unary echo used to prove a transport round-trips.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{
    EchoRequest, EchoResponse, EchoStreamFrame, RpcStreamFrame, SessionFrame, TunnelOpen,
    TunnelOpenAck,
};
use tddy_service::tonic_sandbox::sandbox_service_server::{SandboxService, SandboxServiceServer};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream, UnixListenerStream};
use tonic::{Request, Response, Status, Streaming};

/// What the fake pushes to the host on `SessionChannel`.
#[derive(Clone)]
pub enum Mode {
    /// Echo only — `SessionChannel` pushes nothing.
    EchoOnly,
    /// Push one `ToolRequest` for `tool_name`.
    PushToolRequest { tool_name: String },
    /// Push one `TunnelOpen` for `host:port`.
    PushTunnelOpen { host: String, port: u16 },
    /// Push one `RpcRequest` for `service`/`method` with `payload`, so the host-relay RPC bridge
    /// dispatches it to the injected `HostRpcHandler` and replies with `RpcStreamFrame`s.
    PushRpcRequest {
        request_id: String,
        service: String,
        method: String,
        payload: Vec<u8>,
    },
    /// Play a `--workspace-tools` jail: answer every inbound `in_jail_tool_request` with
    /// `result_json`, recording what was asked and whether the host ever had two calls in flight.
    ServeInJailTools { result_json: String },
    /// Play a jail whose channel dies mid-call: on the first inbound `in_jail_tool_request`, drop
    /// the outbound stream without answering.
    CloseOnInJailToolCall,
    /// Play a jail that takes the call and goes silent — the channel stays open and readable, but
    /// no `in_jail_tool_response` ever comes. What a panicked in-jail executor or a `Shell` stuck
    /// past any sane bound looks like from the host: nothing to notice, only an answer that never
    /// arrives.
    AcceptInJailToolCallsWithoutAnswering,
}

/// Frames the host sent back to the fake.
#[derive(Default)]
pub struct Captured {
    pub tool_responses: Vec<ExecuteToolResponse>,
    pub tunnel_acks: Vec<TunnelOpenAck>,
    pub rpc_stream_frames: Vec<RpcStreamFrame>,
    /// Tool calls the host sent *into* the jail, in arrival order.
    pub in_jail_tool_requests: Vec<ExecuteToolRequest>,
    /// Set if a second `in_jail_tool_request` ever arrived before the previous one was answered —
    /// the frame carries no request id, so that would be unanswerable, not merely eager.
    pub saw_concurrent_in_jail_calls: bool,
}

pub struct FakeSandboxService {
    mode: Mode,
    captured: Arc<Mutex<Captured>>,
}

impl FakeSandboxService {
    pub fn new(mode: Mode) -> (Self, Arc<Mutex<Captured>>) {
        let captured = Arc::new(Mutex::new(Captured::default()));
        (
            Self {
                mode,
                captured: Arc::clone(&captured),
            },
            captured,
        )
    }
}

#[tonic::async_trait]
impl SandboxService for FakeSandboxService {
    type SessionChannelStream = ReceiverStream<Result<SessionFrame, Status>>;
    type EchoStreamStream = ReceiverStream<Result<EchoStreamFrame, Status>>;

    async fn session_channel(
        &self,
        request: Request<Streaming<SessionFrame>>,
    ) -> Result<Response<Self::SessionChannelStream>, Status> {
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(16);
        let mode = self.mode.clone();
        let captured = Arc::clone(&self.captured);

        tokio::spawn(async move {
            match &mode {
                Mode::PushToolRequest { tool_name } => {
                    let _ = tx
                        .send(Ok(SessionFrame {
                            payload: Some(SessionPayload::ToolRequest(ExecuteToolRequest {
                                tool_name: tool_name.clone(),
                                args_json: "{}".to_string(),
                                ..Default::default()
                            })),
                        }))
                        .await;
                }
                Mode::PushTunnelOpen { host, port } => {
                    let _ = tx
                        .send(Ok(SessionFrame {
                            payload: Some(SessionPayload::TunnelOpen(TunnelOpen {
                                tunnel_id: "tunnel-1".to_string(),
                                host: host.clone(),
                                port: *port as u32,
                            })),
                        }))
                        .await;
                }
                Mode::PushRpcRequest {
                    request_id,
                    service,
                    method,
                    payload,
                } => {
                    let _ = tx
                        .send(Ok(SessionFrame {
                            payload: Some(SessionPayload::RpcRequest(
                                tddy_service::proto::sandbox::RpcRequest {
                                    request_id: request_id.clone(),
                                    service: service.clone(),
                                    method: method.clone(),
                                    payload: payload.clone(),
                                },
                            )),
                        }))
                        .await;
                }
                Mode::EchoOnly
                | Mode::ServeInJailTools { .. }
                | Mode::CloseOnInJailToolCall
                | Mode::AcceptInJailToolCallsWithoutAnswering => {}
            }

            // Whether an in-jail tool call is still unanswered. Shared with the spawned answer
            // task rather than held on this stack, because the loop has to keep *reading* while a
            // call is in flight — answering inline would make an overlapping second call
            // impossible to observe, and the guard below unfalsifiable.
            let in_flight = Arc::new(AtomicBool::new(false));
            while let Some(Ok(frame)) = inbound.next().await {
                match frame.payload {
                    Some(SessionPayload::ToolResponse(resp)) => {
                        captured.lock().unwrap().tool_responses.push(resp);
                    }
                    Some(SessionPayload::TunnelOpenAck(ack)) => {
                        captured.lock().unwrap().tunnel_acks.push(ack);
                    }
                    Some(SessionPayload::RpcStreamFrame(frame)) => {
                        captured.lock().unwrap().rpc_stream_frames.push(frame);
                    }
                    Some(SessionPayload::InJailToolRequest(req)) => {
                        {
                            let mut captured = captured.lock().unwrap();
                            if in_flight.load(Ordering::SeqCst) {
                                captured.saw_concurrent_in_jail_calls = true;
                            }
                            captured.in_jail_tool_requests.push(req);
                        }
                        match &mode {
                            Mode::CloseOnInJailToolCall => break,
                            Mode::ServeInJailTools { result_json } => {
                                in_flight.store(true, Ordering::SeqCst);
                                let tx = tx.clone();
                                let in_flight = Arc::clone(&in_flight);
                                let result_json = result_json.clone();
                                // A real jail takes time to run the tool. Answering off the read
                                // loop keeps this end reading meanwhile, so a host that dispatched
                                // a second call early would be seen doing it.
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                    let _ = tx
                                        .send(Ok(SessionFrame {
                                            payload: Some(SessionPayload::InJailToolResponse(
                                                ExecuteToolResponse {
                                                    result_json,
                                                    ..Default::default()
                                                },
                                            )),
                                        }))
                                        .await;
                                    in_flight.store(false, Ordering::SeqCst);
                                });
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        Ok(Response::new(EchoResponse {
            message: request.into_inner().message,
        }))
    }

    async fn echo_stream(
        &self,
        _request: Request<Streaming<EchoStreamFrame>>,
    ) -> Result<Response<Self::EchoStreamStream>, Status> {
        Err(Status::unimplemented("echo_stream not used in tests"))
    }
}

/// Serve the fake on a fresh loopback TCP port; returns the `http://` endpoint.
pub async fn serve_fake_over_tcp(mode: Mode) -> (String, Arc<Mutex<Captured>>) {
    let (svc, captured) = FakeSandboxService::new(mode);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind tcp");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(SandboxServiceServer::new(svc))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await;
    });
    (format!("http://{addr}"), captured)
}

/// Serve the fake on an AF_UNIX socket at `path`.
pub async fn serve_fake_over_uds(path: &Path, mode: Mode) -> Arc<Mutex<Captured>> {
    let (svc, captured) = FakeSandboxService::new(mode);
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).expect("bind uds");
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(SandboxServiceServer::new(svc))
            .serve_with_incoming(UnixListenerStream::new(listener))
            .await;
    });
    captured
}
