//! Acceptance tests: attachable terminal sessions (PRD: docs/ft/daemon/terminal-sessions.md).
//!
//! A tddy session may run multiple tools, each identified. The original `claude` tool is the
//! reserved id `MAIN_TERMINAL_ID` ("main", kind "claude-cli"); started Bash tools (kind "bash" —
//! a `$SHELL` in the worktree, no inputs) get fresh ids. Tools are managed over RPC
//! (start/stop/list) and their I/O is addressed by `terminal_id`.
//!
//! These tests reuse the existing `PtyHandle` mechanic; PTY-spawning tests run serially.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, PtyHandle, MAIN_TERMINAL_ID};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListTerminalSessionsRequest, SessionTerminalInput,
    StartTerminalSessionRequest, StopTerminalSessionRequest, StreamTerminalOutputRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "term-test-session";
/// Stub for the main `claude` terminal: `/bin/cat` stays alive reading PTY stdin.
const MAIN_STUB: &str = "/bin/cat";
/// Login shell used for started terminals in tests.
const SHELL: &str = "/bin/sh";

fn test_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "users:\n  - github_user: \"testuser\"\n    os_user: \"testuser\"\n";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn minimal_service_with_manager(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    )
}

/// Build a service wired to `manager`, returning temp-dir guards that must stay alive.
fn make_service(
    manager: Arc<ClaudeCliSessionManager>,
) -> (ConnectionServiceImpl, tempfile::TempDir, tempfile::TempDir) {
    let (cfg_dir, config) = test_config();
    let sessions = tempfile::tempdir().unwrap();
    let service = minimal_service_with_manager(config, sessions.path().to_path_buf(), manager);
    (service, cfg_dir, sessions)
}

/// Pre-register the main `claude` terminal for `SESSION_ID` in a fresh worktree, returning the
/// worktree temp-dir guard (must stay alive for the worktree to exist).
async fn start_main_terminal(manager: &ClaudeCliSessionManager) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    manager
        .start(
            SESSION_ID,
            worktree.path().to_path_buf(),
            "claude-opus-4-8",
            MAIN_STUB,
            None,
            None,
        )
        .await
        .expect("main claude terminal must start");
    worktree
}

/// Poll `handle.capture` until its UTF-8 contents contain `needle` or the timeout elapses.
async fn wait_for_capture_contains(handle: &Arc<PtyHandle>, needle: &str, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        {
            let cap = handle.capture.lock().unwrap();
            if String::from_utf8_lossy(&cap).contains(needle) {
                return true;
            }
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

// ---------------------------------------------------------------------------
// Manager-level: the PtyHandle mechanic for multiple identified terminals.
// ---------------------------------------------------------------------------

/// **start_terminal_registers_under_fresh_id_distinct_from_main**: starting a Bash tool yields a
/// non-empty id that is not the reserved `MAIN_TERMINAL_ID`, with kind `"bash"`.
#[tokio::test]
#[serial_test::serial]
async fn start_terminal_registers_under_fresh_id_distinct_from_main() {
    // Given
    let manager = ClaudeCliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();

    // When
    let handle = manager
        .start_terminal(SESSION_ID, worktree.path().to_path_buf(), SHELL)
        .await
        .expect("start_terminal must succeed");

    // Then
    assert!(
        !handle.terminal_id.is_empty(),
        "started terminal must have a non-empty id"
    );
    assert_ne!(
        handle.terminal_id, MAIN_TERMINAL_ID,
        "started terminal id must differ from the reserved main id"
    );
    assert_eq!(handle.kind, "bash", "started Bash tool kind must be 'bash'");
}

/// **list_terminals_includes_main_reserved_id_and_started_shell**: after the main terminal plus a
/// started shell, `list_terminals` returns both, identified by id and kind.
#[tokio::test]
#[serial_test::serial]
async fn list_terminals_includes_main_reserved_id_and_started_shell() {
    // Given
    let manager = ClaudeCliSessionManager::new();
    let _worktree = start_main_terminal(&manager).await;
    let shell = manager
        .start_terminal(SESSION_ID, _worktree.path().to_path_buf(), SHELL)
        .await
        .expect("start_terminal must succeed");

    // When
    let terminals = manager.list_terminals(SESSION_ID).await;

    // Then
    let main = terminals
        .iter()
        .find(|h| h.terminal_id == MAIN_TERMINAL_ID)
        .expect("list must include the reserved 'main' terminal");
    assert_eq!(
        main.kind, "claude-cli",
        "main terminal kind must be 'claude-cli'"
    );

    let started = terminals
        .iter()
        .find(|h| h.terminal_id == shell.terminal_id)
        .expect("list must include the started Bash tool");
    assert_eq!(
        started.kind, "bash",
        "started Bash tool kind must be 'bash'"
    );
}

/// **started_terminal_runs_login_shell_in_worktree**: writing `pwd` to a started terminal's stdin
/// produces output containing the worktree path — proving the PTY+stdin+capture mechanic and that
/// the shell runs in the session's worktree.
#[tokio::test]
#[serial_test::serial]
async fn started_terminal_runs_login_shell_in_worktree() {
    // Given
    let manager = ClaudeCliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    let marker = worktree
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let handle = manager
        .start_terminal(SESSION_ID, worktree.path().to_path_buf(), SHELL)
        .await
        .expect("start_terminal must succeed");

    // When — ask the shell to print its working directory.
    let _ = handle.stdin_tx.send(Bytes::from_static(b"pwd\n"));

    // Then — the worktree's unique directory name appears in the captured output.
    let found = wait_for_capture_contains(&handle, &marker, 3000).await;
    assert!(
        found,
        "shell 'pwd' output must contain the worktree dir name '{marker}'"
    );
}

/// **get_terminal_resolves_started_terminal_by_id**: `get_terminal` returns the started terminal
/// for its id, and `None` for an unknown id or a different session.
#[tokio::test]
#[serial_test::serial]
async fn get_terminal_resolves_started_terminal_by_id() {
    // Given
    let manager = ClaudeCliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    let handle = manager
        .start_terminal(SESSION_ID, worktree.path().to_path_buf(), SHELL)
        .await
        .expect("start_terminal must succeed");

    // When / Then
    let got = manager
        .get_terminal(SESSION_ID, &handle.terminal_id)
        .await
        .expect("get_terminal must resolve the started terminal");
    assert_eq!(got.terminal_id, handle.terminal_id);

    assert!(
        manager
            .get_terminal(SESSION_ID, "no-such-id")
            .await
            .is_none(),
        "unknown terminal id must resolve to None"
    );
    assert!(
        manager
            .get_terminal("other-session", &handle.terminal_id)
            .await
            .is_none(),
        "terminal id of a different session must resolve to None"
    );
}

/// **stop_terminal_kills_process_and_deregisters**: `stop_terminal` returns true, removes the
/// terminal from the registry/list, and terminates the underlying process.
#[tokio::test]
#[serial_test::serial]
async fn stop_terminal_kills_process_and_deregisters() {
    // Given
    let manager = ClaudeCliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    let handle = manager
        .start_terminal(SESSION_ID, worktree.path().to_path_buf(), SHELL)
        .await
        .expect("start_terminal must succeed");
    let pid = handle.pid;
    assert!(pid_alive(pid), "process must be alive right after start");

    // When
    let stopped = manager.stop_terminal(SESSION_ID, &handle.terminal_id).await;

    // Then
    assert!(stopped, "stop_terminal must report the terminal existed");
    assert!(
        manager
            .get_terminal(SESSION_ID, &handle.terminal_id)
            .await
            .is_none(),
        "stopped terminal must be removed from the registry"
    );
    assert!(
        !manager
            .list_terminals(SESSION_ID)
            .await
            .iter()
            .any(|h| h.terminal_id == handle.terminal_id),
        "stopped terminal must not appear in list_terminals"
    );

    let mut dead = false;
    for _ in 0..150 {
        if !pid_alive(pid) {
            dead = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(dead, "stopped terminal's process must terminate");
}

// ---------------------------------------------------------------------------
// RPC-level: ConnectionService start/stop/list/identity/auth/I/O routing.
// ---------------------------------------------------------------------------

/// **start_terminal_session_returns_fresh_terminal_id**: the RPC returns a non-empty id distinct
/// from the reserved main id.
#[tokio::test]
#[serial_test::serial]
async fn start_terminal_session_returns_fresh_terminal_id() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When
    let resp = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("StartTerminalSession must succeed");

    // Then
    let terminal_id = resp.into_inner().terminal_id;
    assert!(!terminal_id.is_empty(), "terminal_id must be non-empty");
    assert_ne!(
        terminal_id, MAIN_TERMINAL_ID,
        "started terminal_id must differ from the reserved main id"
    );
}

/// **list_terminal_sessions_returns_main_and_started**: after starting a terminal, the list RPC
/// reports both the reserved main terminal and the started one.
#[tokio::test]
#[serial_test::serial]
async fn list_terminal_sessions_returns_main_and_started() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let started = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("StartTerminalSession must succeed")
        .into_inner()
        .terminal_id;

    // When
    let terminals = service
        .list_terminal_sessions(Request::new(ListTerminalSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("ListTerminalSessions must succeed")
        .into_inner()
        .terminals;

    // Then
    let ids: Vec<String> = terminals.iter().map(|t| t.terminal_id.clone()).collect();
    assert!(
        ids.iter().any(|id| id == MAIN_TERMINAL_ID),
        "list must include the reserved 'main' terminal; got {ids:?}"
    );
    assert!(
        ids.contains(&started),
        "list must include the started terminal '{started}'; got {ids:?}"
    );
}

/// **stop_terminal_session_removes_from_list**: stopping a started terminal removes it from the
/// list RPC's results.
#[tokio::test]
#[serial_test::serial]
async fn stop_terminal_session_removes_from_list() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let started = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("StartTerminalSession must succeed")
        .into_inner()
        .terminal_id;

    // When
    let stop = service
        .stop_terminal_session(Request::new(StopTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: started.clone(),
        }))
        .await
        .expect("StopTerminalSession must succeed");
    assert!(stop.into_inner().ok, "StopTerminalSession must report ok");

    // Then
    let terminals = service
        .list_terminal_sessions(Request::new(ListTerminalSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("ListTerminalSessions must succeed")
        .into_inner()
        .terminals;
    assert!(
        !terminals.iter().any(|t| t.terminal_id == started),
        "stopped terminal must not appear in the list"
    );
}

/// **stop_terminal_session_rejecting_main_returns_invalid_argument**: the main terminal cannot be
/// stopped via this API.
#[tokio::test]
#[serial_test::serial]
async fn stop_terminal_session_rejecting_main_returns_invalid_argument() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When
    let err = service
        .stop_terminal_session(Request::new(StopTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: MAIN_TERMINAL_ID.to_string(),
        }))
        .await
        .expect_err("stopping the main terminal must be rejected");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "stopping 'main' must yield INVALID_ARGUMENT"
    );
}

/// **terminal_session_rpcs_require_valid_token**: start/list/stop all reject an invalid token with
/// UNAUTHENTICATED.
#[tokio::test]
async fn terminal_session_rpcs_require_valid_token() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When / Then — start
    let e1 = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: "bad-token".to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect_err("StartTerminalSession with bad token must fail");
    assert_eq!(e1.code, Code::Unauthenticated);

    // list
    let e2 = service
        .list_terminal_sessions(Request::new(ListTerminalSessionsRequest {
            session_token: "bad-token".to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect_err("ListTerminalSessions with bad token must fail");
    assert_eq!(e2.code, Code::Unauthenticated);

    // stop
    let e3 = service
        .stop_terminal_session(Request::new(StopTerminalSessionRequest {
            session_token: "bad-token".to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: "anything".to_string(),
        }))
        .await
        .expect_err("StopTerminalSession with bad token must fail");
    assert_eq!(e3.code, Code::Unauthenticated);
}

/// **stream_terminal_output_routes_by_terminal_id**: output streaming resolves the addressed
/// terminal — an unknown id is NOT_FOUND, the started terminal's id succeeds.
#[tokio::test]
#[serial_test::serial]
async fn stream_terminal_output_routes_by_terminal_id() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let started = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("StartTerminalSession must succeed")
        .into_inner()
        .terminal_id;

    // When / Then — unknown terminal id is NotFound.
    // (`.err()` avoids requiring `Debug` on the streaming Ok type.)
    let err = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: "no-such-terminal".to_string(),
            initial_cols: 0,
            initial_rows: 0,
        }))
        .await
        .err()
        .expect("streaming an unknown terminal must be an error");
    assert_eq!(
        err.code,
        Code::NotFound,
        "unknown terminal id must be NotFound"
    );

    // The started terminal's id resolves and the stream is established.
    let ok = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: started.clone(),
            initial_cols: 0,
            initial_rows: 0,
        }))
        .await;
    assert!(
        ok.is_ok(),
        "streaming output for a known terminal id must succeed"
    );
}

/// **send_terminal_input_targets_identified_terminal**: input addressed to a started terminal's id
/// reaches that terminal (its capture echoes the marker) and does NOT leak to the main terminal.
#[tokio::test]
#[serial_test::serial]
async fn send_terminal_input_targets_identified_terminal() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    let started = service
        .start_terminal_session(Request::new(StartTerminalSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("StartTerminalSession must succeed")
        .into_inner()
        .terminal_id;

    let marker = "ZZ_SHELL_MARKER_ZZ";

    // When — send input addressed to the started terminal.
    service
        .send_terminal_input(Request::new(SessionTerminalInput {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            data: format!("echo {marker}\n").into_bytes(),
            terminal_id: started.clone(),
        }))
        .await
        .expect("SendTerminalInput must succeed");

    // Then — the started terminal echoes the marker...
    let shell_handle = manager
        .get_terminal(SESSION_ID, &started)
        .await
        .expect("started terminal must exist");
    let found = wait_for_capture_contains(&shell_handle, marker, 3000).await;
    assert!(found, "input must reach the addressed terminal");

    // ...and the main terminal never sees it.
    let main_handle = manager
        .get(SESSION_ID)
        .await
        .expect("main terminal must exist");
    let main_cap = main_handle.capture.lock().unwrap();
    assert!(
        !String::from_utf8_lossy(&main_cap).contains(marker),
        "input addressed to a terminal must not leak to the main terminal"
    );
}
