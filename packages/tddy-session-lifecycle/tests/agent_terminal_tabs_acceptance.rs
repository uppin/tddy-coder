//! Acceptance tests: agent terminal tabs — `StartTerminalSession` naming a CLI harness as its
//! `kind` runs that harness, fresh and interactive, in the session's worktree beside the main
//! agent, under a terminal id of its own.
//!
//! Feature: docs/ft/web/session-terminal-tabs.md, docs/ft/daemon/terminal-sessions.md
//!
//! The harnesses are stub scripts named in `cli_harnesses.<type>.binary_path`, each recording its
//! argv and working directory and then holding its PTY open. PTY-spawning tests run serially.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_session_lifecycle::claude_cli_session::{ClaudeCliSessionManager, MAIN_TERMINAL_ID};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    ListTerminalSessionsRequest, StartTerminalSessionRequest, StopTerminalSessionRequest,
    TerminalSessionInfo, TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::{
    a_stub_agent_script, appended_invocation_count, read_last_appended_argv,
};
use tddy_testing_commons::wait::eventually;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "agent-tab-session";

/// A safety net for a spawned stub to record itself, not a prediction of how long it takes.
const SPAWN_OBSERVE: Duration = Duration::from_secs(10);

/// One stub harness binary and what it records.
struct StubHarness {
    binary: PathBuf,
    argv_log: PathBuf,
    cwd_log: PathBuf,
}

fn a_stub_harness(dir: &Path, name: &str) -> StubHarness {
    let argv_log = dir.join(format!("{name}.argv.log"));
    let cwd_log = dir.join(format!("{name}.cwd.log"));
    let binary = a_stub_agent_script(dir, &format!("{name}.sh"))
        .with_prelude(&format!("pwd -P > \"{}\"", cwd_log.display()))
        .appending_argv_to(&argv_log)
        .then_reading_stdin()
        .build();
    StubHarness {
        binary,
        argv_log,
        cwd_log,
    }
}

impl StubHarness {
    /// The args of this harness's most recent spawn, waiting for the stub to record them.
    async fn last_spawn_args(&self) -> Vec<String> {
        eventually("the stub harness to record a spawn", SPAWN_OBSERVE, || {
            read_last_appended_argv(&self.argv_log)
        })
        .await
    }

    /// The directory this harness last ran in, waiting for the stub to record it.
    async fn last_working_directory(&self) -> PathBuf {
        eventually("the stub harness to record its cwd", SPAWN_OBSERVE, || {
            std::fs::read_to_string(&self.cwd_log)
                .map(|cwd| PathBuf::from(cwd.trim()))
                .map_err(|e| format!("{} is not readable yet: {e}", self.cwd_log.display()))
        })
        .await
    }

    fn was_spawned(&self) -> bool {
        appended_invocation_count(&self.argv_log) > 0
    }
}

/// A daemon serving one running session whose main agent holds a worktree, with stub Muse Code,
/// OpenCode and Claude Code binaries configured as those harnesses' binaries.
struct AgentTabDaemon {
    service: tddy_terminal_rpc::TerminalSessionServiceImpl,
    worktree: PathBuf,
    muse: StubHarness,
    opencode: StubHarness,
    claude: StubHarness,
    _worktree_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
}

async fn a_daemon_with_a_running_session() -> AgentTabDaemon {
    let stub_dir = tempfile::tempdir().unwrap();
    let muse = a_stub_harness(stub_dir.path(), "muse");
    let opencode = a_stub_harness(stub_dir.path(), "opencode");
    let claude = a_stub_harness(stub_dir.path(), "claude");
    let (config_dir, config) = a_config_naming(&[
        ("muse-cli", &muse.binary),
        ("opencode-cli", &opencode.binary),
        ("claude-cli", &claude.binary),
    ]);

    let manager = Arc::new(ClaudeCliSessionManager::new());
    let worktree_dir = tempfile::tempdir().unwrap();
    let worktree = worktree_dir.path().canonicalize().unwrap();
    start_main_terminal(&manager, &worktree).await;

    let sessions_dir = tempfile::tempdir().unwrap();
    let service =
        session_host(config, sessions_dir.path().to_path_buf(), manager).terminal_session_service();
    AgentTabDaemon {
        service,
        worktree,
        muse,
        opencode,
        claude,
        _worktree_dir: worktree_dir,
        _stub_dir: stub_dir,
        _config_dir: config_dir,
        _sessions_dir: sessions_dir,
    }
}

impl AgentTabDaemon {
    async fn start_terminal(&self, kind: &str) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    async fn terminals(&self) -> Vec<TerminalSessionInfo> {
        self.service
            .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .expect("ListTerminalSessions must succeed")
            .into_inner()
            .terminals
    }

    async fn stop_terminal(&self, terminal_id: &str) {
        self.service
            .stop_terminal_session(Request::direct(StopTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("StopTerminalSession must succeed");
    }
}

fn the_value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

// ---------------------------------------------------------------------------
// Starting an agent terminal
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn an_agent_terminal_runs_that_harness_in_the_sessions_worktree() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon
        .start_terminal("muse-cli")
        .await
        .expect("a Muse tab starts");

    // Then Muse Code is started fresh — no model, no prompt — where the main agent works
    assert_eq!(daemon.muse.last_spawn_args().await, Vec::<String>::new());
    assert_eq!(daemon.muse.last_working_directory().await, daemon.worktree);
}

#[tokio::test]
#[serial_test::serial]
async fn an_agent_terminal_is_listed_with_its_harness_as_its_kind() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;
    let terminal_id = daemon.start_terminal("muse-cli").await.unwrap();

    // When
    let terminals = daemon.terminals().await;

    // Then
    let tab = terminals
        .iter()
        .find(|t| t.terminal_id == terminal_id)
        .expect("the agent terminal is listed");
    assert_eq!(tab.kind, "muse-cli");
    assert!(tab.pid > 0);
}

#[tokio::test]
#[serial_test::serial]
async fn an_agent_terminal_gets_a_fresh_id_distinct_from_main_and_from_other_tabs() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let first = daemon.start_terminal("muse-cli").await.unwrap();
    let second = daemon.start_terminal("muse-cli").await.unwrap();

    // Then
    assert_ne!(first, MAIN_TERMINAL_ID);
    assert_ne!(second, MAIN_TERMINAL_ID);
    assert_ne!(first, second);
    let muse_tabs: Vec<String> = daemon
        .terminals()
        .await
        .into_iter()
        .filter(|t| t.kind == "muse-cli")
        .map(|t| t.terminal_id)
        .collect();
    assert_eq!(muse_tabs.len(), 2, "{muse_tabs:?}");
    assert!(muse_tabs.contains(&first) && muse_tabs.contains(&second));
}

#[tokio::test]
#[serial_test::serial]
async fn an_agent_terminal_resolves_its_binary_from_the_harness_config() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon.start_terminal("opencode-cli").await.unwrap();

    // Then the binary `cli_harnesses.opencode-cli.binary_path` names is the one run
    assert_eq!(
        daemon.opencode.last_spawn_args().await,
        Vec::<String>::new()
    );
    assert!(!daemon.muse.was_spawned(), "the wrong harness was spawned");
}

#[tokio::test]
#[serial_test::serial]
async fn a_claude_agent_terminal_starts_a_conversation_of_its_own() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon.start_terminal("claude-cli").await.unwrap();

    // Then Claude Code is given a session id of the tab's own, never the session's
    let argv = daemon.claude.last_spawn_args().await;
    let conversation = the_value_after(&argv, "--session-id").expect("a --session-id");
    assert!(!conversation.is_empty());
    assert_ne!(conversation, SESSION_ID);
}

// ---------------------------------------------------------------------------
// Refusals and stopping
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn an_unknown_terminal_kind_is_refused_and_nothing_is_spawned() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let refusal = daemon
        .start_terminal("emacs")
        .await
        .expect_err("emacs is no CLI harness");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(refusal.message.contains("emacs"), "{}", refusal.message);
    let ids: Vec<String> = daemon
        .terminals()
        .await
        .into_iter()
        .map(|t| t.terminal_id)
        .collect();
    assert_eq!(ids, vec![MAIN_TERMINAL_ID.to_string()]);
}

#[tokio::test]
#[serial_test::serial]
async fn stopping_an_agent_terminal_kills_it_and_removes_it_from_the_list() {
    // Given
    let daemon = a_daemon_with_a_running_session().await;
    let terminal_id = daemon.start_terminal("muse-cli").await.unwrap();
    let tab = daemon
        .terminals()
        .await
        .into_iter()
        .find(|t| t.terminal_id == terminal_id)
        .expect("the agent terminal is listed");
    assert_eq!(tab.kind, "muse-cli");
    let pid = tab.pid;

    // When
    daemon.stop_terminal(&terminal_id).await;

    // Then
    let ids: Vec<String> = daemon
        .terminals()
        .await
        .into_iter()
        .map(|t| t.terminal_id)
        .collect();
    assert!(!ids.contains(&terminal_id), "{ids:?}");
    eventually(
        "the stopped agent's process to exit",
        SPAWN_OBSERVE,
        || match pid_alive(pid) {
            false => Ok(()),
            true => Err(format!("pid {pid} is still alive")),
        },
    )
    .await;
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn a_config_naming(harnesses: &[(&str, &Path)]) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let entries: String = harnesses
        .iter()
        .map(|(session_type, binary)| {
            format!("  {session_type}:\n    binary_path: {}\n", binary.display())
        })
        .collect();
    let yaml = format!(
        "users:\n  - github_user: \"testuser\"\n    os_user: \"testuser\"\ncli_harnesses:\n{entries}"
    );
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}

/// Registers the session's main terminal — a stub that holds its PTY open — in `worktree`.
async fn start_main_terminal(manager: &ClaudeCliSessionManager, worktree: &Path) {
    let main = a_stub_agent_script(worktree, "stub_main.sh")
        .then_reading_stdin()
        .build();
    manager
        .start(
            SESSION_ID,
            worktree.to_path_buf(),
            "claude-opus-4-8",
            main.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("main terminal must start");
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
