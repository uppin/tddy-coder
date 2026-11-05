//! Acceptance: a `tddy-tools submit` a headless agent runs from its own shell reaches the session
//! with no `TDDY_*` variable in its environment, through the `.tddy/session.json` pointer the
//! backend writes in the agent's working directory (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`. The command stays the one
//! every workflow prompt already names, so no prompt changes.

#![cfg(unix)]

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;

/// The session's toolcall listener, standing in for the daemon's: records each request's
/// method and JSON body, and acknowledges it.
#[derive(Clone, Default)]
struct ARecordingListener {
    received: Arc<Mutex<Vec<(String, Value)>>>,
}

#[async_trait::async_trait]
impl tddy_rpc::RpcService for ARecordingListener {
    async fn handle_rpc(
        &self,
        _service: &str,
        method: &str,
        message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        let body: Value = serde_json::from_slice(&message.payload).unwrap_or(Value::Null);
        let goal = body["goal"].clone();
        self.received
            .lock()
            .expect("received")
            .push((method.to_string(), body));
        let answer = serde_json::json!({ "status": "ok", "goal": goal });
        tddy_rpc::RpcResult::Unary(Ok(answer.to_string().into_bytes()))
    }
}

/// A worktree with the session pointer at its root naming `socket`, and a directory below it.
fn a_worktree_pointing_at(socket: &std::path::Path) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().expect("worktree");
    std::fs::create_dir_all(worktree.path().join(".tddy")).expect(".tddy");
    std::fs::create_dir_all(worktree.path().join("src")).expect("src");
    std::fs::write(
        worktree.path().join(".tddy/session.json"),
        serde_json::json!({
            "socket": socket.to_string_lossy(),
            "repo_dir": worktree.path().to_string_lossy(),
        })
        .to_string(),
    )
    .expect("pointer");
    worktree
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shell_run_tddy_tools_submit_reaches_the_session_through_the_pointer_with_no_tddy_env() {
    // Given — the session listening on its socket, and an agent shell in a directory below the
    // pointer whose environment has no `TDDY_*` variable
    let sockets = tempfile::tempdir().expect("sockets");
    let socket = sockets.path().join("toolcall.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let session = ARecordingListener::default();
    let server = {
        let session = session.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (reader, writer) = stream.into_split();
            let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                reader,
                writer,
                session,
                tddy_rpc::RequestTransport::UnixSocket,
            );
            endpoint.run().await;
        })
    };
    let worktree = a_worktree_pointing_at(&socket);
    let payload =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    // When — the command every workflow prompt names, run from the agent's shell
    let output = tokio::task::spawn_blocking({
        let cwd = worktree.path().join("src");
        move || {
            let mut cmd = cargo_bin_cmd!("tddy-tools");
            for (name, _) in std::env::vars() {
                if name.starts_with("TDDY_") {
                    cmd.env_remove(name);
                }
            }
            cmd.current_dir(cwd)
                .args(["submit", "--goal", "plan", "--data", payload])
                .output()
                .expect("run tddy-tools submit")
        }
    })
    .await
    .expect("submit ran");
    server.abort();

    // Then — the session received the submission, and the agent was told so
    let received: Vec<(String, Value)> = session.received.lock().expect("received").clone();
    let acknowledged: Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap_or(Value::Null);
    assert_eq!(
        (
            output.status.code(),
            received
                .iter()
                .map(|(_, body)| body["goal"].clone())
                .collect::<Vec<_>>(),
            acknowledged["status"].clone(),
        ),
        (
            Some(0),
            vec![Value::String("plan".to_string())],
            Value::String("ok".to_string())
        ),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
