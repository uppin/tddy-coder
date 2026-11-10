//! Acceptance tests: a session's agent terminal output outlives its process — `GetTerminalHistory`
//! for `main` of a stopped session answers from the recording in the session directory, also for a
//! fresh daemon over the same data dir, while a running `main` always answers from its live ring.
//!
//! Feature: docs/ft/daemon/terminal-sessions.md § History of a stopped agent terminal
//!
//! The agent is a stub that prints a line, waits for a newline on stdin and exits. Its terminal is
//! registered for recording at the session directory's path the way the launcher registers it.
//! PTY-spawning tests run serially.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::session_dir_recorded_history::main_capture_recording_path;
use tddy_task::CaptureRecording;
use tddy_terminal_rpc::proto::terminal_session::{
    GetTerminalHistoryRequest, StreamTerminalOutputRequest, TerminalHistoryChunk,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::eventually_awaiting;
use tokio_stream::StreamExt;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "recorded-agent-session";

/// A safety net for the stub to print and exit, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

/// A daemon over a data dir the test owns, whose manager records running terminals every 50ms.
struct RecordingDaemon {
    service: tddy_terminal_rpc::TerminalSessionServiceImpl,
    manager: Arc<CliSessionManager>,
    data_dir: PathBuf,
    worktree: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

fn a_daemon() -> RecordingDaemon {
    let data_dir = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let (config_dir, config) = a_config();
    let manager =
        Arc::new(CliSessionManager::new().with_recording_interval(Duration::from_millis(50)));
    let service = session_host(config, data_dir.path().to_path_buf(), Arc::clone(&manager))
        .terminal_session_service();
    RecordingDaemon {
        service,
        manager,
        data_dir: data_dir.path().to_path_buf(),
        worktree: worktree.path().canonicalize().unwrap(),
        _dirs: vec![data_dir, worktree, config_dir],
    }
}

impl RecordingDaemon {
    /// A second daemon over this one's data dir — what a restart leaves.
    fn restarted(&self) -> tddy_terminal_rpc::TerminalSessionServiceImpl {
        let (_config_dir, config) = a_config();
        session_host(
            config,
            self.data_dir.clone(),
            Arc::new(CliSessionManager::new()),
        )
        .terminal_session_service()
    }

    /// Start the session's agent printing `first`, recorded where the launcher records it; on a
    /// newline it prints `last` and exits.
    async fn with_a_recorded_agent_printing(&self, first: &str, last: &str) -> RunningAgent {
        let agent = self.worktree.join("stub_agent.sh");
        std::fs::write(
            &agent,
            format!("#!/bin/sh\nprintf '{first}'\nread _line\nprintf '{last}'\nexit 0\n"),
        )
        .unwrap();
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
        // The recorder creates the session's `terminals/` directory itself.
        let recording = main_capture_recording_path(&self.data_dir, SESSION_ID);
        let main = self
            .manager
            .start_with_options(
                SESSION_ID,
                self.worktree.clone(),
                "claude-opus-4-8",
                vec![agent.to_str().unwrap().to_string()],
                "claude-cli",
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("main terminal must start");
        self.manager
            .record_main_capture(SESSION_ID, recording)
            .await;
        RunningAgent { main }
    }

    async fn until_main_has_stopped(&self) {
        eventually_awaiting("main to stop", OBSERVE, || async {
            match self.manager.get(SESSION_ID).await {
                None => Ok(()),
                Some(_) => Err("main is still running".to_string()),
            }
        })
        .await;
    }
}

struct RunningAgent {
    main: Arc<tddy_session_lifecycle::claude_cli_session::PtyHandle>,
}

impl RunningAgent {
    fn let_it_exit(&self) {
        self.main.stdin_tx.send(Bytes::from_static(b"\n")).unwrap();
    }

    async fn until_it_printed(&self, text: &str) {
        let capture = Arc::clone(&self.main.capture);
        eventually_awaiting("the agent to print", OBSERVE, || {
            let printed =
                String::from_utf8_lossy(capture.lock().unwrap().buffered_bytes()).contains(text);
            async move {
                printed
                    .then_some(())
                    .ok_or_else(|| "not printed yet".to_string())
            }
        })
        .await;
    }
}

fn a_history_request(from: u64, until: u64, max: u32) -> GetTerminalHistoryRequest {
    GetTerminalHistoryRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: "main".to_string(),
        from_offset: from,
        until_offset: until,
        max_bytes: max,
    }
}

async fn history(
    service: &tddy_terminal_rpc::TerminalSessionServiceImpl,
    request: GetTerminalHistoryRequest,
) -> Result<Vec<TerminalHistoryChunk>, tddy_rpc::Status> {
    let stream = service
        .get_terminal_history(Request::direct(request))
        .await?
        .into_inner();
    stream.collect::<Result<Vec<_>, _>>().await
}

/// The whole retained history of `main` as text, read through `GetTerminalHistory`.
async fn all_history_text(service: &tddy_terminal_rpc::TerminalSessionServiceImpl) -> String {
    let chunks = history(service, a_history_request(0, 0, 64 * 1024))
        .await
        .expect("GetTerminalHistory answers");
    String::from_utf8_lossy(&chunks.concat_data()).into_owned()
}

trait ConcatData {
    fn concat_data(&self) -> Vec<u8>;
}

impl ConcatData for Vec<TerminalHistoryChunk> {
    fn concat_data(&self) -> Vec<u8> {
        self.iter().flat_map(|chunk| chunk.data.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// A stopped session's agent terminal
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn main_history_is_served_after_the_agent_process_exits() {
    // Given an agent that printed and exited
    let daemon = a_daemon();
    let agent = daemon
        .with_a_recorded_agent_printing("working", "last-words")
        .await;
    agent.until_it_printed("working").await;
    agent.let_it_exit();
    daemon.until_main_has_stopped().await;

    // When its history is read
    let text = all_history_text(&daemon.service).await;

    // Then it holds everything the agent printed
    assert!(
        text.contains("working") && text.contains("last-words"),
        "{text:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn main_history_is_served_by_a_fresh_daemon_over_the_same_session_dir() {
    // Given an agent that printed and exited under one daemon
    let daemon = a_daemon();
    let agent = daemon
        .with_a_recorded_agent_printing("before-restart", "gone")
        .await;
    agent.until_it_printed("before-restart").await;
    agent.let_it_exit();
    daemon.until_main_has_stopped().await;

    // When a restarted daemon reads its history
    let text = all_history_text(&daemon.restarted()).await;

    // Then the recording answers
    assert!(
        text.contains("before-restart") && text.contains("gone"),
        "{text:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn paged_reads_over_a_recording_match_the_live_capture_byte_for_byte() {
    // Given the pages a running agent's history answers, up to an anchor
    let daemon = a_daemon();
    let agent = daemon
        .with_a_recorded_agent_printing("0123456789abcdef", "z")
        .await;
    agent.until_it_printed("0123456789abcdef").await;
    let anchor = agent.main.capture.lock().unwrap().end_offset();
    let mut live_pages = Vec::new();
    for from in (0..anchor).step_by(4) {
        live_pages.push(
            history(&daemon.service, a_history_request(from, anchor, 4))
                .await
                .unwrap(),
        );
    }

    // When the agent has exited and the same pages are read again
    agent.let_it_exit();
    daemon.until_main_has_stopped().await;
    let mut recorded_pages = Vec::new();
    for from in (0..anchor).step_by(4) {
        recorded_pages.push(
            history(&daemon.service, a_history_request(from, anchor, 4))
                .await
                .unwrap(),
        );
    }

    // Then every page is identical, offsets and flags included
    assert_eq!(recorded_pages, live_pages);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_running_main_serves_its_live_capture_not_the_recording() {
    // Given a stale recording on disk and a running agent printing something else
    let daemon = a_daemon();
    let stale = main_capture_recording_path(&daemon.data_dir, SESSION_ID);
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    CaptureRecording {
        start_offset: 0,
        end_offset: 5,
        data: b"stale".to_vec(),
    }
    .write_atomically(&stale)
    .expect("the stale recording is written");
    let agent = daemon
        .with_a_recorded_agent_printing("live-output", "bye")
        .await;
    agent.until_it_printed("live-output").await;

    // When its history is read
    let text = all_history_text(&daemon.service).await;

    // Then the live ring answers
    assert!(
        text.contains("live-output") && !text.contains("stale"),
        "{text:?}"
    );
    agent.let_it_exit();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn streaming_a_stopped_main_is_still_not_found() {
    // Given an agent that printed and exited
    let daemon = a_daemon();
    let agent = daemon
        .with_a_recorded_agent_printing("working", "done")
        .await;
    agent.until_it_printed("working").await;
    agent.let_it_exit();
    daemon.until_main_has_stopped().await;

    // When a client opens its output stream
    let Err(refusal) = daemon
        .service
        .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
            ..Default::default()
        }))
        .await
    else {
        panic!("a stopped terminal has no output stream");
    };

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_session_that_never_recorded_answers_not_found() {
    // Given a daemon with no running and no recorded terminal for the session
    let daemon = a_daemon();

    // When its history is read
    let refusal = history(&daemon.service, a_history_request(0, 0, 1024))
        .await
        .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn a_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, "").unwrap();
    std::fs::write(
        std::path::Path::new(&(path))
            .parent()
            .expect("a config directory")
            .join("users.yaml"),
        "- github_user: \"testuser\"\n  os_user: \"testuser\"\n",
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn session_host(
    config: DaemonConfig,
    data_dir: PathBuf,
    manager: Arc<CliSessionManager>,
) -> DaemonSessionHost {
    let sessions_base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    )
}
