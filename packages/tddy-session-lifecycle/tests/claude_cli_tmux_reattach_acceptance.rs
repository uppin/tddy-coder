//! Acceptance tests: a tmux-run Claude CLI agent survives the daemon and is re-attached, not
//! relaunched (`#tmux-agent` 2/3, docs/ft/daemon/claude-cli-session.md § tmux mode).
//!
//! The fixtures mirror `claude_cli_tmux_session_acceptance.rs` (TODO(tmux-agent): share them through
//! `tests/support/` once the `#tmux-agent` stack has landed — both files are stack-shared), plus a
//! *restart*: a second daemon over the same sessions directory and the same private tmux server.
//! Serial — the tmux socket directory is process-wide environment.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_last_appended_argv};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use support::{this_os_user, until_attached, PrivateTmux, OBSERVE, VALID_TOKEN};
use tddy_terminal_rpc::session::TerminalSessionStore;

const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "tmux-agent-project";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A daemon whose `claude` is a stub that records what it was launched with, then holds its pane.
struct TmuxAgentDaemon {
    host: Arc<DaemonSessionHost>,
    manager: Arc<CliSessionManager>,
    config: AConfigAndItsAgents,
    sessions_base: PathBuf,
    launches: PathBuf,
    tmux: PrivateTmux,
    _dirs: Vec<tempfile::TempDir>,
    _repo: tempfile::TempDir,
}

async fn a_daemon_with_a_private_tmux() -> TmuxAgentDaemon {
    a_daemon_over(CliSessionManager::new()).await
}

async fn a_daemon_over(manager: CliSessionManager) -> TmuxAgentDaemon {
    let tmux = PrivateTmux::started();
    let sessions_dir = tempfile::tempdir().unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let launches = stub_dir.path().join("launches.log");
    let stub = a_stub_agent_script(stub_dir.path(), "stub_claude.sh")
        .appending_argv_to(&launches)
        .then_reading_stdin()
        .build();
    let repo = a_registered_project(sessions_dir.path());
    let (config_dir, config) = a_config_running(&stub);
    let manager = Arc::new(manager);
    let host = a_session_host(
        config.clone(),
        sessions_dir.path().to_path_buf(),
        Arc::clone(&manager),
    );
    TmuxAgentDaemon {
        host,
        manager,
        config,
        sessions_base: sessions_dir.path().to_path_buf(),
        launches,
        tmux,
        _dirs: vec![sessions_dir, stub_dir, config_dir],
        _repo: repo,
    }
}

/// A daemon config, and the coding agents the host installs: `claude` is the stub.
type AConfigAndItsAgents = (
    DaemonConfig,
    Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
);

fn a_config_running(stub_agent: &Path) -> (tempfile::TempDir, AConfigAndItsAgents) {
    let dir = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\n"
    );
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    let agents =
        tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[("claude", stub_agent)]);
    (dir, (config, agents))
}

fn a_session_host(
    (config, agents): AConfigAndItsAgents,
    sessions_base: PathBuf,
    manager: Arc<CliSessionManager>,
) -> Arc<DaemonSessionHost> {
    let data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user = this_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| user.clone()));
    let host = Arc::new(DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
        agents,
    ));
    host.install_sandbox_rpc_bridge();
    host
}

/// A git repo with an `origin`, registered as the test project under `sessions_base/projects`.
fn a_registered_project(sessions_base: &Path) -> tempfile::TempDir {
    let repo_dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo_dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", repo_dir.path().to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
    let projects_dir = sessions_base.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: {TEST_PROJECT_ID}\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo_dir.path().display()
        ),
    )
    .unwrap();
    repo_dir
}

/// A managed claude-cli start (managed codebase with a recipe): the session gets its toolcall
/// listener, the socket the client-exit and agent-exit tests follow.
fn a_claude_cli_start_request(run_in_tmux: bool, sandbox: bool) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        run_in_tmux,
        sandbox,
        managed_codebase: true,
        recipe: "tdd".to_string(),
        ..Default::default()
    }
}

impl TmuxAgentDaemon {
    async fn starts_a_session_in_tmux(&self) -> String {
        self.host
            .start_session(Request::direct(a_claude_cli_start_request(true, false)))
            .await
            .expect("StartSession with run_in_tmux must succeed")
            .into_inner()
            .session_id
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    fn metadata_of(&self, session_id: &str) -> tddy_core::SessionMetadata {
        tddy_core::read_session_metadata(&self.session_dir(session_id))
            .expect("the session's .session.yaml is readable")
    }

    /// The tmux session the daemon recorded for `session_id`, once it has recorded one.
    fn tmux_session_of(&self, session_id: &str) -> String {
        self.metadata_of(session_id)
            .tmux_session
            .expect("a tmux-run session records its tmux session")
    }

    async fn until_the_agent_ran_in_tmux_session(&self, tmux: &PrivateTmux, name: &str) {
        eventually(
            "the stub agent to be launched in its tmux session",
            OBSERVE,
            || {
                read_last_appended_argv(&self.launches)?;
                tmux.has_session(name)
                    .then_some(())
                    .ok_or_else(|| format!("tmux has no session {name:?} yet"))
            },
        )
        .await;
    }
}

fn a_process_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

impl TmuxAgentDaemon {
    /// A daemon restarted over the same sessions directory: no terminals registered, same tmux.
    fn restarted(&self) -> Arc<DaemonSessionHost> {
        a_session_host(
            self.config.clone(),
            self.sessions_base.clone(),
            Arc::new(CliSessionManager::new()),
        )
    }

    /// How many times the stub agent has been launched.
    fn launch_count(&self) -> usize {
        std::fs::read_to_string(&self.launches)
            .map(|log| log.lines().count())
            .unwrap_or(0)
    }

    /// The toolcall socket the session's tool server was told to use (`TDDY_SOCKET`, in the `env` of
    /// the `tddy-tools` registration the agent loads — the agent's own environment carries no
    /// `TDDY_*`).
    async fn toolcall_socket(&self, session_id: &str) -> PathBuf {
        let registration = self.session_dir(session_id).join("claude-mcp-config.json");
        let seen = eventually("the session to register its tool server", OBSERVE, || {
            let text = std::fs::read_to_string(&registration)
                .map_err(|e| format!("{} is not readable yet: {e}", registration.display()))?;
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("not JSON yet: {e}"))?;
            json["mcpServers"]["tddy-tools"]["env"]["TDDY_SOCKET"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "the registration carries no TDDY_SOCKET".to_string())
        })
        .await;
        PathBuf::from(seen)
    }

    async fn is_running_in_tmux(&self) -> (String, u32) {
        let session_id = self.starts_a_session_in_tmux().await;
        let name = self.tmux_session_of(&session_id);
        self.until_the_agent_ran_in_tmux_session(&self.tmux, &name)
            .await;
        let pane_pid = self.metadata_of(&session_id).pid.expect("a recorded pid");
        (session_id, pane_pid)
    }

    async fn until_the_main_left_the_registry(&self, session_id: &str) {
        eventually_awaiting(
            "the main terminal to leave the registry",
            OBSERVE,
            || async {
                match self.manager.get(session_id).await {
                    None => Ok(()),
                    Some(_) => Err("the main terminal is still registered".to_string()),
                }
            },
        )
        .await;
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_daemon_shutdown_leaves_a_tmux_run_agent_running() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, agent_pid) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);

    // When the daemon shuts down
    daemon.host.shut_down_children().await;

    // Then the agent and its tmux session are still running, and no client is attached
    assert!(daemon.tmux.has_session(&name), "the tmux session survives");
    assert!(a_process_is_alive(agent_pid), "the agent survives");
    until_attached(&daemon.tmux, &name, 0).await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_fresh_daemon_reattaches_the_main_terminal_on_first_stream_without_restarting_the_agent()
{
    // Given a tmux-run session that outlived its daemon
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, agent_pid) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.host.shut_down_children().await;
    let fresh = daemon.restarted();

    // When a client first streams the session's main terminal from the fresh daemon
    let streamed = fresh
        .terminal_session_service()
        .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
            terminal_id: MAIN_TERMINAL_ID.to_string(),
            initial_cols: 0,
            initial_rows: 0,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            control_token: String::new(),
        }))
        .await;

    // Then a client is attached to the same tmux session and the agent was not relaunched
    assert!(
        streamed.is_ok(),
        "the first stream re-attaches the main terminal"
    );
    assert!(
        fresh
            .terminal_sessions()
            .get_terminal(&session_id, MAIN_TERMINAL_ID)
            .await
            .is_some(),
        "the main terminal is registered"
    );
    until_attached(&daemon.tmux, &name, 1).await;
    assert_eq!(daemon.launch_count(), 1, "claude was not relaunched");
    assert_eq!(daemon.metadata_of(&session_id).pid, Some(agent_pid));
}

#[tokio::test]
#[serial_test::serial]
async fn resume_of_a_live_tmux_run_session_reattaches_instead_of_relaunching() {
    // Given a tmux-run session that outlived its daemon
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, agent_pid) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.host.shut_down_children().await;
    let fresh = daemon.restarted();

    // When it is resumed
    fresh
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession");

    // Then the same tmux session has a client again, and claude was not launched a second time
    until_attached(&daemon.tmux, &name, 1).await;
    assert_eq!(daemon.tmux_session_of(&session_id), name);
    assert_eq!(daemon.launch_count(), 1, "claude was not relaunched");
    assert_eq!(
        daemon.metadata_of(&session_id).pid,
        Some(agent_pid),
        "the pane's pid is re-recorded"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn resume_of_a_tmux_run_session_whose_tmux_session_is_gone_starts_fresh() {
    // Given a tmux-run session whose daemon is gone and whose tmux session was killed meanwhile
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.host.shut_down_children().await;
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());
    let fresh = daemon.restarted();

    // When it is resumed
    fresh
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession");

    // Then claude is launched again with the resume argv, in a tmux session with a client
    let resumed = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &resumed, 1).await;
    let argv = eventually("claude to be relaunched with --resume", OBSERVE, || {
        let argv = read_last_appended_argv(&daemon.launches)?;
        argv.contains(&"--resume".to_string())
            .then_some(argv.clone())
            .ok_or_else(|| format!("last launch: {argv:?}"))
    })
    .await;
    assert!(argv.windows(2).any(|w| w == ["--resume", &session_id]));
    assert_eq!(daemon.launch_count(), 2);
}

#[tokio::test]
#[serial_test::serial]
async fn a_client_exit_while_tmux_lives_keeps_the_sessions_toolcall_socket() {
    // Given a running tmux-run session, and the socket its agent was told to use
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let socket = daemon.toolcall_socket(&session_id).await;
    assert!(socket.exists(), "the socket exists while the session runs");

    // When the client exits (a detach) while tmux lives
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    daemon.until_the_main_left_the_registry(&session_id).await;

    // Then the agent's tmux session and the toolcall socket are still there
    assert!(daemon.tmux.has_session(&name));
    assert!(socket.exists(), "the toolcall socket outlives the client");
}

#[tokio::test]
#[serial_test::serial]
async fn resuming_a_live_session_on_the_same_daemon_keeps_its_toolcall_socket_answering() {
    // Given a running tmux-run session whose client then exits (a detach)
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let socket = daemon.toolcall_socket(&session_id).await;
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    daemon.until_the_main_left_the_registry(&session_id).await;

    // When the session is resumed on the same daemon
    daemon
        .host
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession");

    // Then the socket the agent was told to use still exists and answers
    until_attached(&daemon.tmux, &name, 1).await;
    assert!(
        socket.exists(),
        "the resume did not unlink the agent's socket"
    );
    assert!(
        std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "the agent's toolcall socket answers"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_detached_agents_end_runs_the_cleanup_when_its_watcher_notices() {
    // Given a tmux-run session whose client has exited, watched every 50 ms
    let daemon = a_daemon_over(
        CliSessionManager::new().with_detach_watch_interval(std::time::Duration::from_millis(50)),
    )
    .await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let socket = daemon.toolcall_socket(&session_id).await;
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    daemon.until_the_main_left_the_registry(&session_id).await;
    assert!(socket.exists(), "a detached agent keeps its socket");

    // When its tmux session is killed from outside
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());

    // Then the watcher runs the agent-ended cleanup
    eventually("the toolcall socket to be removed", OBSERVE, || {
        (!socket.exists())
            .then_some(())
            .ok_or_else(|| format!("{} still exists", socket.display()))
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn an_agent_exit_runs_the_main_exit_cleanup_and_records_history() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let socket = daemon.toolcall_socket(&session_id).await;

    // When the agent exits (its tmux session ends)
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());
    daemon.until_the_main_left_the_registry(&session_id).await;

    // Then the session's workflow is cleaned up and its main history was recorded
    eventually("the toolcall socket to be removed", OBSERVE, || {
        (!socket.exists())
            .then_some(())
            .ok_or_else(|| format!("{} still exists", socket.display()))
    })
    .await;
    let recording = tddy_cli_sessions::cli_session_manager::main_capture_recording_path(
        &daemon.sessions_base,
        &session_id,
    );
    assert!(recording.exists(), "the main's history was recorded");
}

// ---------------------------------------------------------------------------
// Re-attach reads the installed coding agents (`#agent-tools 17/19`)
// ---------------------------------------------------------------------------

impl TmuxAgentDaemon {
    /// `<tddyhome>/tools` now holds a file that is no tool definition, so a registry loaded from it
    /// is refused — what the directory looks like when an operator's override is mid-edit. The
    /// coding agents a daemon was *given* are unaffected.
    fn has_a_tools_dir_that_no_longer_loads(&self) {
        let tools = self.sessions_base.join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("half-written.yaml"), "schema_version: [1\n").unwrap();
    }

    /// `session_id`'s `.session.yaml` no longer records a session type — a record written before
    /// session types were recorded, or one damaged since.
    fn forgets_the_session_type_of(&self, session_id: &str) {
        let mut meta = self.metadata_of(session_id);
        meta.session_type = None;
        tddy_core::write_session_metadata(&self.session_dir(session_id), &meta).unwrap();
    }
}

/// A client's first stream of `session_id`'s main terminal on `host`, answered or refused.
async fn first_stream_of_the_main_terminal(
    host: &DaemonSessionHost,
    session_id: &str,
) -> Result<(), tddy_rpc::Status> {
    host.terminal_session_service()
        .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
            terminal_id: MAIN_TERMINAL_ID.to_string(),
            initial_cols: 0,
            initial_rows: 0,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            control_token: String::new(),
        }))
        .await
        .map(|_| ())
}

/// Whether `host` has a main terminal registered for `session_id`.
async fn has_a_main_terminal(host: &DaemonSessionHost, session_id: &str) -> bool {
    host.terminal_sessions()
        .get_terminal(session_id, MAIN_TERMINAL_ID)
        .await
        .is_some()
}

#[tokio::test]
#[serial_test::serial]
async fn a_daemon_restart_reattaches_a_tmux_claude_cli_session_from_the_installed_coding_agents() {
    // Given a tmux-run claude-cli session that outlived its daemon, and a restarted daemon given its
    // coding agents while `<tddyhome>/tools` holds a file that no longer loads
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, agent_pid) = daemon.is_running_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.host.shut_down_children().await;
    let fresh = daemon.restarted();
    daemon.has_a_tools_dir_that_no_longer_loads();

    // When a client first streams the session's main terminal from the fresh daemon
    let streamed = first_stream_of_the_main_terminal(&fresh, &session_id).await;

    // Then the main is re-attached from the installed agents — `<tddyhome>/tools` is not read —
    // and the agent was not relaunched
    assert_eq!(
        (
            streamed.map_err(|status| status.message),
            has_a_main_terminal(&fresh, &session_id).await,
            daemon.launch_count(),
            daemon.metadata_of(&session_id).pid,
        ),
        (Ok(()), true, 1, Some(agent_pid))
    );
    until_attached(&daemon.tmux, &name, 1).await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_recorded_session_without_a_session_type_is_not_reattached() {
    // Given a tmux-run session that outlived its daemon, whose record no longer names its session
    // type
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _) = daemon.is_running_in_tmux().await;
    daemon.host.shut_down_children().await;
    daemon.forgets_the_session_type_of(&session_id);
    let fresh = daemon.restarted();

    // When a client first streams the session's main terminal from the fresh daemon
    let _refused = first_stream_of_the_main_terminal(&fresh, &session_id).await;

    // Then nothing is re-attached as a default agent, and nothing is relaunched
    assert_eq!(
        (
            has_a_main_terminal(&fresh, &session_id).await,
            daemon.launch_count(),
        ),
        (false, 1)
    );
}
