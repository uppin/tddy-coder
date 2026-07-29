//! Acceptance tests: input-offset acknowledgement on the terminal output stream.
//!
//! PRD: docs/ft/web/enqueued-input-overlay.md
//!
//! The client tags each `SendTerminalInput` with a cumulative byte `input_offset`. After the
//! daemon writes those bytes to the PTY it must acknowledge the applied offset by streaming a
//! `SessionTerminalOutput { data: [], acked_input_offset: N }` frame on the already-open
//! `StreamTerminalOutput` stream. The applied offset is monotonic (max wins), so a later input
//! carrying a smaller offset never lowers the acknowledged value.
//!
//! These tests reuse the main-terminal stub mechanic and run serially (they spawn a real PTY).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, MAIN_TERMINAL_ID};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Request, Status};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, SessionTerminalInput, SessionTerminalOutput,
    StreamReplayMode, StreamTerminalOutputRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "ack-test-session";
const POLL: Duration = Duration::from_millis(50);

fn test_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "users:\n  - github_user: \"testuser\"\n    os_user: \"testuser\"\n";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn make_service(
    manager: Arc<ClaudeCliSessionManager>,
) -> (ConnectionServiceImpl, tempfile::TempDir, tempfile::TempDir) {
    let (cfg_dir, config) = test_config();
    let sessions = tempfile::tempdir().unwrap();
    let sessions_base = sessions.path().to_path_buf();
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    );
    (service, cfg_dir, sessions)
}

fn write_main_stub(dir: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("stub_main.sh");
    std::fs::write(&script, "#!/bin/sh\nexec cat\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

async fn start_main_terminal(manager: &ClaudeCliSessionManager) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    let stub = write_main_stub(worktree.path());
    manager
        .start(
            SESSION_ID,
            worktree.path().to_path_buf(),
            "claude-opus-4-8",
            stub.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("main claude terminal must start");
    worktree
}

fn a_stream_request() -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
        initial_cols: 80,
        initial_rows: 24,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
    }
}

fn an_input(bytes: &str, input_offset: u64) -> SessionTerminalInput {
    SessionTerminalInput {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        data: bytes.as_bytes().to_vec(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
        control_token: String::new(),
        input_offset,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
        initial_cols: 0,
        initial_rows: 0,
    }
}

/// Collect every non-zero `acked_input_offset` seen on the stream within `deadline`.
async fn collect_acks<S>(mut stream: S, deadline: Duration) -> Vec<u64>
where
    S: StreamExt<Item = Result<SessionTerminalOutput, Status>> + Unpin,
{
    let mut acks = Vec::new();
    let end = tokio::time::Instant::now() + deadline;
    while tokio::time::Instant::now() < end {
        if let Ok(Some(Ok(msg))) = tokio::time::timeout(POLL, stream.next()).await {
            if msg.acked_input_offset > 0 {
                acks.push(msg.acked_input_offset);
            }
        }
    }
    acks
}

/// **acks the applied input offset on the output stream**: after `SendTerminalInput` with
/// `input_offset = N`, an output frame reports `acked_input_offset == N`.
#[tokio::test]
#[serial_test::serial]
async fn acks_the_applied_input_offset_on_the_output_stream() {
    // Given — a running main terminal with an open output stream
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let stream = service
        .stream_terminal_output(Request::new(a_stream_request()))
        .await
        .expect("StreamTerminalOutput must succeed")
        .into_inner();

    // When — input is sent tagged with cumulative offset 42
    service
        .send_terminal_input(Request::new(an_input("echo hi\n", 42)))
        .await
        .expect("SendTerminalInput must succeed");

    // Then — the stream carries an ACK for offset 42
    let acks = collect_acks(stream, Duration::from_millis(3000)).await;
    assert!(
        acks.contains(&42),
        "expected an ACK for applied offset 42, saw {acks:?}"
    );
}

/// **never lowers the acked offset for a later smaller offset**: applied offset is monotonic —
/// after acking 100, a later input carrying offset 50 must not produce an ACK below 100.
#[tokio::test]
#[serial_test::serial]
async fn never_lowers_the_acked_offset_for_a_later_smaller_offset() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let stream = service
        .stream_terminal_output(Request::new(a_stream_request()))
        .await
        .expect("StreamTerminalOutput must succeed")
        .into_inner();

    // When — a higher offset is applied, then a lower one arrives
    service
        .send_terminal_input(Request::new(an_input("a", 100)))
        .await
        .expect("first SendTerminalInput must succeed");
    service
        .send_terminal_input(Request::new(an_input("b", 50)))
        .await
        .expect("second SendTerminalInput must succeed");

    // Then — 100 is acked and no ACK ever regresses below it
    let acks = collect_acks(stream, Duration::from_millis(3000)).await;
    assert!(
        acks.contains(&100),
        "expected an ACK for offset 100, saw {acks:?}"
    );
    assert!(
        !acks.iter().any(|&o| o < 100),
        "applied offset must be monotonic; saw a regression in {acks:?}"
    );
}
