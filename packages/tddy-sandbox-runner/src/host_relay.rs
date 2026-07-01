//! Host-side `SessionChannel` driver, shared by the daemon, the standalone app, and tests.
//!
//! The host is a dumb byte relay: it answers `HostPoll`, fulfills CONNECT tunnels by opening the
//! real outbound socket and pumping bytes both ways (TLS stays end-to-end — the host never sees
//! plaintext), relays legacy unary egress, and forwards PTY output to a sink. Tool execution is
//! injected via [`HostToolHandler`] so each caller supplies its own behavior (the daemon runs real
//! tools; tests stub them).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

use tddy_service::proto::connection::ExecuteToolResponse;
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{
    EgressRequest, EgressResponse, HostPoll, SandboxInput, SessionFrame, SubscribeTerminal,
    TunnelClose, TunnelData, TunnelOpen, TunnelOpenAck,
};

use crate::runner::SandboxClient;

/// One-shot "the pty session ended" flag, race-free regardless of whether `signal()` or `wait()`
/// runs first (the classic check-notify-check-await pattern for `tokio::sync::Notify`).
struct EndSignal {
    ended: AtomicBool,
    notify: tokio::sync::Notify,
}

impl EndSignal {
    fn new() -> Self {
        Self {
            ended: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    fn signal(&self) {
        self.ended.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            if self.ended.load(Ordering::SeqCst) {
                return;
            }
            let notified = self.notify.notified();
            if self.ended.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// Injected tool-execution behavior for the host side of a sandbox `SessionChannel`.
#[async_trait]
pub trait HostToolHandler: Send + Sync + 'static {
    /// Execute a tool requested by the in-jail agent and return its response.
    async fn execute(
        &self,
        session_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse;
}

/// Wiring for [`run_host_relay`].
pub struct HostRelayConfig {
    /// Session id used on outbound frames (e.g. `SubscribeTerminal`).
    pub session_id: String,
    /// PTY output bytes from the jail are forwarded here (the daemon fans into broadcast+capture;
    /// tests collect them into a buffer).
    pub terminal_sink: mpsc::UnboundedSender<Bytes>,
    /// Initial terminal dimensions sent with `SubscribeTerminal`.
    pub initial_cols: u32,
    pub initial_rows: u32,
}

impl HostRelayConfig {
    /// Config with the default 80x24 terminal size (suitable for headless callers and tests).
    pub fn new(session_id: impl Into<String>, terminal_sink: mpsc::UnboundedSender<Bytes>) -> Self {
        Self {
            session_id: session_id.into(),
            terminal_sink,
            initial_cols: 80,
            initial_rows: 24,
        }
    }
}

/// Open the in-jail `SessionChannel` over `client`, subscribe to the main terminal, and drive the
/// host side: poll, relay CONNECT tunnels and egress, forward terminal output to
/// [`HostRelayConfig::terminal_sink`], and dispatch tool requests to `tool_handler`. Returns the
/// background task driving inbound frames; `stdin_rx` bytes are written to the jail PTY.
pub async fn run_host_relay<H: HostToolHandler>(
    mut client: SandboxClient,
    tool_handler: H,
    config: HostRelayConfig,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<JoinHandle<()>, String> {
    let (host_tx, host_rx) = mpsc::channel(64);
    let host_stream = ReceiverStream::new(host_rx);
    let mut session = client
        .session_channel(host_stream)
        .await
        .map_err(|e| format!("open session channel: {e}"))?
        .into_inner();

    host_tx
        .send(SessionFrame {
            payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                session_id: config.session_id.clone(),
                terminal_id: "main".to_string(),
                initial_cols: config.initial_cols,
                initial_rows: config.initial_rows,
            })),
        })
        .await
        .map_err(|_| "session channel closed before subscribe".to_string())?;

    let session_id = config.session_id.clone();
    let terminal_sink = config.terminal_sink;
    let host_tx_reader = host_tx.clone();
    let end_signal = Arc::new(EndSignal::new());

    let reader = tokio::spawn({
        let end_signal = Arc::clone(&end_signal);
        async move {
            // CONNECT tunnels: tunnel_id → sender feeding agent→host bytes into the outbound TCP socket.
            let mut tunnels: HashMap<String, mpsc::UnboundedSender<Bytes>> = HashMap::new();
            while let Some(Ok(frame)) = session.next().await {
                match frame.payload {
                    Some(SessionPayload::SessionEnded(_)) => {
                        // The pty command exited — stop polling and let both ends of the stream drop,
                        // so the in-jail gRPC server can finish shutting down (see `signal_session_ended`).
                        end_signal.signal();
                        break;
                    }
                    Some(SessionPayload::ToolRequest(req)) => {
                        let resp = tool_handler
                            .execute(&session_id, &req.tool_name, &req.args_json)
                            .await;
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::ToolResponse(resp)),
                            })
                            .await;
                    }
                    Some(SessionPayload::EgressRequest(req)) => {
                        let resp = relay_egress_request(req).await;
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::EgressResponse(resp)),
                            })
                            .await;
                    }
                    Some(SessionPayload::TunnelOpen(open)) => {
                        // Agent issued CONNECT host:port — the host owns the real outbound socket.
                        let (tcp_in_tx, tcp_in_rx) = mpsc::unbounded_channel::<Bytes>();
                        tunnels.insert(open.tunnel_id.clone(), tcp_in_tx);
                        spawn_tunnel(open, tcp_in_rx, host_tx_reader.clone());
                    }
                    Some(SessionPayload::TunnelData(data)) => {
                        // Agent→host bytes: feed into the outbound socket for this tunnel.
                        if let Some(tx) = tunnels.get(&data.tunnel_id) {
                            if tx.send(Bytes::from(data.data)).is_err() {
                                tunnels.remove(&data.tunnel_id);
                            }
                        }
                    }
                    Some(SessionPayload::TunnelClose(close)) => {
                        // Agent closed its end: drop the sender so the socket writer shuts down.
                        tunnels.remove(&close.tunnel_id);
                    }
                    Some(SessionPayload::TerminalOutput(out)) => {
                        if !out.data.is_empty() {
                            let _ = terminal_sink.send(Bytes::from(out.data));
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    let session_id_in = config.session_id.clone();
    tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_millis(25));
        // Keep polling even after the caller drops its stdin sender — a closed stdin must not stop
        // `HostPoll` (which drives terminal output and poll-gated frames). Polling does stop once
        // the pty session has ended (`end_signal`), so both ends of the stream can drop and the
        // in-jail gRPC server can finish shutting down.
        let mut stdin_open = true;
        loop {
            tokio::select! {
                _ = end_signal.wait() => break,
                chunk = stdin_rx.recv(), if stdin_open => {
                    match chunk {
                        Some(chunk) => {
                            let _ = host_tx.send(SessionFrame {
                                payload: Some(SessionPayload::TerminalInput(SandboxInput {
                                    session_id: session_id_in.clone(),
                                    terminal_id: "main".to_string(),
                                    data: chunk.to_vec(),
                                })),
                            }).await;
                        }
                        None => stdin_open = false,
                    }
                }
                _ = poll.tick() => {
                    if host_tx.send(SessionFrame {
                        payload: Some(SessionPayload::HostPoll(HostPoll {})),
                    }).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(reader)
}

/// Open the real outbound TCP connection for a relayed `CONNECT` tunnel and pump bytes both ways
/// over the `SessionChannel`. The host is a dumb byte relay — TLS stays end-to-end between the
/// in-jail agent and the target, so credentials never appear in plaintext here.
fn spawn_tunnel(
    open: TunnelOpen,
    mut tcp_in_rx: mpsc::UnboundedReceiver<Bytes>,
    host_tx: mpsc::Sender<SessionFrame>,
) {
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let tunnel_id = open.tunnel_id.clone();
        let addr = format!("{}:{}", open.host, open.port);
        let stream = match tokio::net::TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                let _ = host_tx
                    .send(SessionFrame {
                        payload: Some(SessionPayload::TunnelOpenAck(TunnelOpenAck {
                            tunnel_id,
                            ok: false,
                            error: format!("connect {addr}: {e}"),
                        })),
                    })
                    .await;
                return;
            }
        };
        let _ = host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::TunnelOpenAck(TunnelOpenAck {
                    tunnel_id: tunnel_id.clone(),
                    ok: true,
                    error: String::new(),
                })),
            })
            .await;

        let (mut read_half, mut write_half) = stream.into_split();

        // host → agent: forward outbound-socket bytes as TunnelData; signal close on EOF/error.
        let up_tx = host_tx.clone();
        let up_id = tunnel_id.clone();
        let up = tokio::spawn(async move {
            let mut buf = [0u8; 16384];
            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if up_tx
                            .send(SessionFrame {
                                payload: Some(SessionPayload::TunnelData(TunnelData {
                                    tunnel_id: up_id.clone(),
                                    data: buf[..n].to_vec(),
                                })),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = up_tx
                .send(SessionFrame {
                    payload: Some(SessionPayload::TunnelClose(TunnelClose {
                        tunnel_id: up_id,
                        error: String::new(),
                    })),
                })
                .await;
        });

        // agent → host: drain inbound bytes into the outbound socket until the agent closes.
        while let Some(bytes) = tcp_in_rx.recv().await {
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
        up.abort();
    });
}

/// Perform a legacy unary egress request on the host's behalf (used for `GET /probe`).
pub async fn relay_egress_request(req: EgressRequest) -> EgressResponse {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return EgressResponse {
                request_id: req.request_id,
                error_message: format!("build http client: {e}"),
                ..Default::default()
            };
        }
    };

    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &req.url);
    for header in &req.headers {
        builder = builder.header(&header.name, &header.value);
    }
    if !req.body.is_empty() {
        builder = builder.body(req.body.clone());
    }

    match builder.send().await {
        Ok(resp) => {
            let status_code = resp.status().as_u16() as u32;
            let body = resp.bytes().await.unwrap_or_default();
            EgressResponse {
                request_id: req.request_id,
                status_code,
                body: body.to_vec(),
                ..Default::default()
            }
        }
        Err(e) => EgressResponse {
            request_id: req.request_id,
            error_message: format!("outbound fetch failed: {e}"),
            ..Default::default()
        },
    }
}

/// Tool handler that rejects all in-jail tool requests (generic confined actions).
pub struct NullToolHandler;

#[async_trait]
impl HostToolHandler for NullToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> ExecuteToolResponse {
        ExecuteToolResponse {
            is_error: true,
            error_message: format!("tools unsupported in generic pty action: {tool_name}"),
            ..Default::default()
        }
    }
}
