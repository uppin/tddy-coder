//! Acceptance tests: a Claude CLI session started with `run_in_tmux` runs its agent inside a tmux
//! session on the host, as the session's OS user (`#tmux-agent` 1/3,
//! docs/ft/daemon/claude-cli-session.md § tmux mode).
//!
//! Real tmux, privately: `TMUX_TMPDIR` points at a fresh socket directory (see `support`), the
//! identity maps to the OS user running the test, and the agent is a stub `claude` that records how
//! it was launched. Serial — the tmux socket directory is process-wide environment.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    DeleteSessionRequest, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::stub_scripts::{
    a_stub_agent_script, read_last_appended_argv, read_recorded_env,
};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use support::{this_os_user, until_attached, PrivateTmux, OBSERVE, VALID_TOKEN};

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
    manager: Arc<ClaudeCliSessionManager>,
    sessions_base: PathBuf,
    launches: PathBuf,
    environment: PathBuf,
    working_directory: PathBuf,
    tmux: PrivateTmux,
    _dirs: Vec<tempfile::TempDir>,
    _repo: tempfile::TempDir,
}

async fn a_daemon_with_a_private_tmux() -> TmuxAgentDaemon {
    a_daemon_whose_agent(AgentTemperament::HoldsItsPane).await
}

/// How the stub `claude` behaves when its tmux session is killed (tmux sends SIGHUP to the pane).
enum AgentTemperament {
    HoldsItsPane,
    IgnoresHangup,
}

async fn a_daemon_whose_agent(temperament: AgentTemperament) -> TmuxAgentDaemon {
    let tmux = PrivateTmux::started();
    let sessions_dir = tempfile::tempdir().unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let launches = stub_dir.path().join("launches.log");
    let environment = stub_dir.path().join("environment.txt");
    let working_directory = stub_dir.path().join("cwd.txt");
    let mut stub = a_stub_agent_script(stub_dir.path(), "stub_claude.sh");
    if matches!(temperament, AgentTemperament::IgnoresHangup) {
        // An ignored signal stays ignored across `exec`, so the `cat` the stub ends in ignores it too.
        stub = stub.with_prelude("trap '' HUP");
    }
    let stub = stub
        .with_prelude(&format!("pwd -P > \"{}\"", working_directory.display()))
        .recording_env_to(&environment, &["CLAUDE_CODE_NO_FLICKER"])
        .appending_argv_to(&launches)
        .then_reading_stdin()
        .build();
    let repo = a_registered_project(sessions_dir.path());
    let (config_dir, config) = a_config_running(&stub);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_dir.path().to_path_buf(),
        Arc::clone(&manager),
    );
    TmuxAgentDaemon {
        host,
        manager,
        sessions_base: sessions_dir.path().to_path_buf(),
        launches,
        environment,
        working_directory,
        tmux,
        _dirs: vec![sessions_dir, stub_dir, config_dir],
        _repo: repo,
    }
}

fn a_config_running(stub_agent: &Path) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\nclaude_cli:\n  binary_path: {}\n",
        stub_agent.display()
    );
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn a_session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
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

fn a_claude_cli_start_request(run_in_tmux: bool, sandbox: bool) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        run_in_tmux,
        sandbox,
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

    /// The worktree the session's agent was cut into.
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("the session records its worktree"),
        )
        .canonicalize()
        .unwrap()
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

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_claude_session_started_in_tmux_runs_claude_in_a_named_tmux_session_in_the_worktree() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then claude runs in the tmux session the daemon named for this session, in its worktree
    let name = tddy_cli_sessions::tmux::agent_tmux_session_name(&session_id);
    assert_eq!(daemon.tmux_session_of(&session_id), name);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    assert_eq!(
        daemon.tmux.pane_directory(&name),
        daemon.worktree_of(&session_id)
    );
    let argv = read_last_appended_argv(&daemon.launches).unwrap();
    assert!(
        argv.windows(2).any(|w| w == ["--session-id", &session_id]),
        "claude is launched with the session id: {argv:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_main_terminal_of_a_tmux_run_session_is_attached_to_its_tmux_session() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then its main terminal is a tmux client of that session
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let main = daemon
        .manager
        .get(&session_id)
        .await
        .expect("the main terminal is registered");
    assert_eq!(main.tmux_session.as_deref(), Some(name.as_str()));
}

#[tokio::test]
#[serial_test::serial]
async fn a_tmux_run_session_records_the_pane_pid_and_reads_active() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then the recorded pid is the agent's pane process, not the tmux client
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let pane_pid: u32 = String::from_utf8_lossy(
        &daemon
            .tmux
            .tmux(&["display-message", "-p", "-t", &name, "#{pane_pid}"])
            .stdout,
    )
    .trim()
    .parse()
    .expect("tmux reports the pane pid");
    let recorded = daemon.metadata_of(&session_id).pid;
    assert_eq!(recorded, Some(pane_pid));
    let client_pid = daemon.manager.get(&session_id).await.unwrap().pid;
    assert_ne!(recorded, Some(client_pid), "pid is not the tmux client's");
    // and it reads active
    let listed = daemon
        .host
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner()
        .sessions;
    let entry = listed
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("the session is listed");
    assert!(entry.is_active, "a tmux-run session reads active");
}

#[tokio::test]
#[serial_test::serial]
async fn claude_in_tmux_sees_the_session_environment() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then the pane's environment carries what the daemon launches an agent with — tmux panes
    // inherit the server's environment, not the client's, so it has to be passed to the pane
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let seen = eventually("the stub to record its environment", OBSERVE, || {
        read_recorded_env(&daemon.environment)
    })
    .await;
    assert_eq!(
        seen.get("CLAUDE_CODE_NO_FLICKER").map(String::as_str),
        Some("1")
    );
    assert!(
        daemon.working_directory.exists(),
        "the stub ran (and recorded its directory)"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_pane_environment_file_is_gone_once_the_agent_runs_and_no_value_was_on_tmuxs_argv() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;

    // Then the pane sourced and deleted the file the environment was handed over in
    let env_dir = daemon
        .session_dir(&session_id)
        .join(tddy_cli_sessions::tmux::pane_env::PANE_ENV_DIR);
    eventually("the pane environment file to be deleted", OBSERVE, || {
        let left: Vec<_> = std::fs::read_dir(&env_dir)
            .map_err(|e| format!("{}: {e}", env_dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        left.is_empty()
            .then_some(())
            .ok_or_else(|| format!("still there: {left:?}"))
    })
    .await;
    // and the pane's command names a file, never a value
    let start_command = daemon.tmux.tmux(&[
        "display-message",
        "-p",
        "-t",
        &format!("={name}:"),
        "#{pane_start_command}",
    ]);
    let start_command = String::from_utf8_lossy(&start_command.stdout);
    assert!(
        !start_command.contains("CLAUDE_CODE_NO_FLICKER"),
        "{start_command}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_tmux_run_session_kills_the_agent_and_the_tmux_session() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let agent_pid = daemon.metadata_of(&session_id).pid.expect("a recorded pid");

    // When it is deleted
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession");

    // Then the agent and its tmux session are gone
    eventually(
        "the agent and its tmux session to be gone",
        OBSERVE,
        || match (
            a_process_is_alive(agent_pid),
            daemon.tmux.has_session(&name),
        ) {
            (false, false) => Ok(()),
            (agent, tmux) => Err(format!("agent alive: {agent}, tmux session alive: {tmux}")),
        },
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn resuming_a_terminated_tmux_run_session_starts_a_fresh_tmux_session_with_the_resume_argv() {
    // Given a tmux-run session whose agent has been terminated
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());
    eventually_awaiting(
        "the main terminal to leave the registry",
        OBSERVE,
        || async {
            match daemon.manager.get(&session_id).await {
                None => Ok(()),
                Some(_) => Err("the main terminal is still registered".to_string()),
            }
        },
    )
    .await;

    // When it is resumed
    daemon
        .host
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession");

    // Then a fresh tmux session runs claude with the resume argv
    let resumed_name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &resumed_name, 1).await;
    let argv = eventually("claude to be relaunched with --resume", OBSERVE, || {
        let argv = read_last_appended_argv(&daemon.launches)?;
        if argv.contains(&"--resume".to_string()) {
            Ok(argv)
        } else {
            Err(format!("last launch: {argv:?}"))
        }
    })
    .await;
    assert!(
        argv.windows(2).any(|w| w == ["--resume", &session_id]),
        "{argv:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn run_in_tmux_with_sandbox_is_refused() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let refusal = daemon
        .host
        .start_session(Request::direct(a_claude_cli_start_request(true, true)))
        .await
        .expect_err("run_in_tmux with sandbox must be refused");

    // Then
    assert_eq!(
        refusal.code(),
        Code::InvalidArgument,
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains("tmux"),
        "the refusal names the option: {}",
        refusal.message()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_tmux_run_session_ends_an_agent_that_ignores_hangup() {
    // Given a running tmux-run session whose agent ignores SIGHUP, the signal kill-session sends
    let daemon = a_daemon_whose_agent(AgentTemperament::IgnoresHangup).await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let agent_pid = daemon.metadata_of(&session_id).pid.expect("a recorded pid");

    // When it is deleted
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession");

    // Then the agent is gone, not merely detached from a dead tmux session
    eventually(
        "the agent to be gone",
        std::time::Duration::from_secs(20),
        || match a_process_is_alive(agent_pid) {
            false => Ok(()),
            true => Err(format!("agent pid {agent_pid} is still alive")),
        },
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_whose_metadata_is_unreadable_still_kills_its_tmux_session() {
    // Given a running tmux-run session whose .session.yaml is then lost
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    std::fs::remove_file(daemon.session_dir(&session_id).join(".session.yaml")).unwrap();

    // When it is deleted
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession");

    // Then the tmux session, named by the session id, is gone
    eventually("the tmux session to be gone", OBSERVE, || {
        match daemon.tmux.has_session(&name) {
            false => Ok(()),
            true => Err(format!("tmux session {name:?} is still alive")),
        }
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn daemon_shutdown_detaches_a_tmux_run_sessions_main_and_leaves_its_agent_running() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let agent_pid = daemon.metadata_of(&session_id).pid.expect("a recorded pid");

    // When the daemon shuts down
    daemon.manager.kill_all().await;

    // Then the main's client is detached, and the agent and its tmux session keep running
    eventually("the main's tmux client to detach", OBSERVE, || {
        (daemon.tmux.attached_clients(&name) == 0)
            .then_some(())
            .ok_or_else(|| "a client is still attached".to_string())
    })
    .await;
    assert!(daemon.tmux.has_session(&name), "the tmux session survives");
    assert!(a_process_is_alive(agent_pid), "the agent survives");
}

#[test]
#[serial_test::serial]
fn a_tmux_session_reads_alive_while_it_runs_and_not_alive_once_killed() {
    use tddy_cli_sessions::tmux::{kill_tmux_session, tmux_session_alive};

    // Given a running tmux session of this user — the liveness probe a re-attach decides on
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let user = this_os_user();
    assert!(tmux_session_alive(&user, "alpha"));

    // When it is killed
    kill_tmux_session(&user, "alpha").expect("kill-session");

    // Then it no longer reads alive, and killing it again is not an error
    assert!(!tmux.has_session("alpha"));
    assert!(!tmux_session_alive(&user, "alpha"));
    kill_tmux_session(&user, "alpha").expect("killing an absent session is not an error");
}
