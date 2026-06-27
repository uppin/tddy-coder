//! Host-side `SessionChannel` driver for sandbox acceptance tests.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tddy_service::proto::connection::ExecuteToolResponse;
use tddy_service::tonic_sandbox::session_frame::Payload as SessionPayload;
use tddy_service::tonic_sandbox::{
    EgressRequest, EgressResponse, HostPoll, SessionFrame, SubscribeTerminal,
};

/// Host client that mirrors daemon `dial_and_bridge` for tests.
pub struct SandboxSessionChannelHost {
    terminal: Arc<Mutex<String>>,
    _reader: tokio::task::JoinHandle<()>,
    _poller: tokio::task::JoinHandle<()>,
}

impl SandboxSessionChannelHost {
    /// Dial the in-jail sandbox gRPC server and start HostPoll relay loops.
    pub async fn connect(ready_marker: &Path, session_id: &str) -> Self {
        let mut client = tddy_tools::sandbox_runner::connect_sandbox_client(ready_marker)
            .await
            .expect("connect sandbox grpc");

        let (host_tx, host_rx) = mpsc::channel(64);
        let host_stream = ReceiverStream::new(host_rx);
        let mut session = client
            .session_channel(host_stream)
            .await
            .expect("SessionChannel must open")
            .into_inner();

        host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                    session_id: session_id.to_string(),
                    terminal_id: "main".to_string(),
                    initial_cols: 80,
                    initial_rows: 24,
                })),
            })
            .await
            .expect("subscribe frame");

        let terminal = Arc::new(Mutex::new(String::new()));
        let terminal_reader = Arc::clone(&terminal);
        let host_tx_reader = host_tx.clone();

        let reader = tokio::spawn(async move {
            while let Some(Ok(frame)) = session.next().await {
                match frame.payload {
                    Some(SessionPayload::TerminalOutput(out)) => {
                        if !out.data.is_empty() {
                            terminal_reader
                                .lock()
                                .await
                                .push_str(&String::from_utf8_lossy(&out.data));
                        }
                    }
                    Some(SessionPayload::EgressRequest(req)) => {
                        let resp = relay_egress_request(req).await;
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::EgressResponse(resp)),
                            })
                            .await;
                    }
                    Some(SessionPayload::ToolRequest(req)) => {
                        let resp = ExecuteToolResponse {
                            result_json: format!(r#"{{"tool":"{}"}}"#, req.tool_name),
                            is_error: false,
                            ..Default::default()
                        };
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::ToolResponse(resp)),
                            })
                            .await;
                    }
                    _ => {}
                }
            }
        });

        let host_tx_poll = host_tx.clone();
        let poller = tokio::spawn(async move {
            let mut poll = tokio::time::interval(Duration::from_millis(25));
            loop {
                poll.tick().await;
                if host_tx_poll
                    .send(SessionFrame {
                        payload: Some(SessionPayload::HostPoll(HostPoll {})),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            terminal,
            _reader: reader,
            _poller: poller,
        }
    }

    /// Wait until terminal output contains `needle` or the deadline expires.
    pub async fn collect_terminal_until(&self, deadline: Duration, needle: &str) -> String {
        let end = tokio::time::Instant::now() + deadline;
        loop {
            {
                let text = self.terminal.lock().await.clone();
                if text.contains(needle) {
                    return text;
                }
                if tokio::time::Instant::now() >= end {
                    return text;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

async fn relay_egress_request(req: EgressRequest) -> EgressResponse {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
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
