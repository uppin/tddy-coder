//! Host-side `SessionChannel` driver for sandbox acceptance tests.
//!
//! Thin wrapper over the shared [`tddy_sandbox_runner::run_host_relay`] with a stub tool handler
//! and a string terminal sink, so tests collect PTY output and assert on it.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_sandbox_runner::{run_host_relay, ExecuteToolResponse, HostRelayConfig, HostToolHandler};
use tokio::sync::{mpsc, Mutex};

/// Stub tool handler: echoes the requested tool name back as the result.
struct StubToolHandler;

#[async_trait::async_trait]
impl HostToolHandler for StubToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> ExecuteToolResponse {
        ExecuteToolResponse {
            result_json: format!(r#"{{"tool":"{tool_name}"}}"#),
            is_error: false,
            ..Default::default()
        }
    }
}

/// Host client that drives the in-jail sandbox `SessionChannel` for tests.
pub struct SandboxSessionChannelHost {
    terminal: Arc<Mutex<String>>,
    _stdin_tx: mpsc::UnboundedSender<Bytes>,
    _relay: tokio::task::JoinHandle<()>,
    _collector: tokio::task::JoinHandle<()>,
}

impl SandboxSessionChannelHost {
    /// Dial the in-jail sandbox gRPC server and drive the host side via the shared relay.
    pub async fn connect(ready_marker: &Path, session_id: &str) -> Self {
        let client = tddy_sandbox_darwin::connect_sandbox_client(ready_marker)
            .await
            .expect("connect sandbox grpc");

        let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel::<Bytes>();
        let terminal = Arc::new(Mutex::new(String::new()));
        let collector_terminal = Arc::clone(&terminal);
        let collector = tokio::spawn(async move {
            while let Some(chunk) = terminal_rx.recv().await {
                collector_terminal
                    .lock()
                    .await
                    .push_str(&String::from_utf8_lossy(&chunk));
            }
        });

        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let relay = run_host_relay(
            client,
            StubToolHandler,
            HostRelayConfig::new(session_id, terminal_tx),
            stdin_rx,
        )
        .await
        .expect("run host relay");

        Self {
            terminal,
            _stdin_tx: stdin_tx,
            _relay: relay,
            _collector: collector,
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
