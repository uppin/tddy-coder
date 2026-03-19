//! ACP agent stub for testing ClaudeAcpBackend without real Claude.
//!
//! Communicates over stdio using the Agent Client Protocol. Reads scenario from
//! `--scenario <path>` or `TDDY_ACP_SCENARIO` env var. Default: echo prompt back as text.

use std::fs;
use std::path::PathBuf;

use agent_client_protocol::{self as acp, Client};

use agent::StubAgent;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

mod agent;
mod scenario;

use scenario::Scenario;

fn main() -> std::io::Result<()> {
    let scenario_path = std::env::args()
        .skip(1)
        .find(|a| a == "--scenario")
        .and_then(|_| std::env::args().nth(2))
        .or_else(|| std::env::var("TDDY_ACP_SCENARIO").ok())
        .map(PathBuf::from);

    let scenario = scenario_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Scenario>(&s).ok())
        .unwrap_or_default();

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let local_set = tokio::task::LocalSet::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(local_set.run_until(async move {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<agent::AgentMessage>();
        let agent = StubAgent::with_channel(tx, scenario);
        let (conn, handle_io) = acp::AgentSideConnection::new(agent, outgoing, incoming, |fut| {
            tokio::task::spawn_local(fut);
        });

        tokio::task::spawn_local(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    agent::AgentMessage::SessionNotification(notif, tx) => {
                        let _ = conn.session_notification(notif).await;
                        tx.send(()).ok();
                    }
                    agent::AgentMessage::RequestPermission(req, tx) => {
                        let result = conn.request_permission(req).await;
                        tx.send(result).ok();
                    }
                }
            }
        });

        handle_io.await
    }))
    .map_err(std::io::Error::other)
}
