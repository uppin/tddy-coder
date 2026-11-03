//! Acceptance tests: every CLI harness session — cursor-cli, opencode-cli, muse-cli, warp-cli —
//! can run its agent in tmux as the session's OS user (`#tmux-agent` 3/3,
//! docs/ft/daemon/1-WIP/PRD-2026-11-03-harnesses-in-tmux.md).
//!
//! The fixtures mirror `claude_cli_tmux_session_acceptance.rs` and
//! `claude_cli_tmux_reattach_acceptance.rs` (n1's and n2's, read-only here); the harnesses are
//! stub scripts named in `cli_harnesses.<type>.binary_path` (cursor: `cursor_cli.binary_path`),
//! the pattern of `agent_terminal_tabs_acceptance.rs`. Serial — the tmux socket directory is
//! process-wide environment.
//!
//! Every start goes through n1's launcher and every resume through n2's re-attach, so these stay
//! red until n1 and n2 are green.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_last_appended_argv};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use support::{this_os_user, until_attached, PrivateTmux, OBSERVE, VALID_TOKEN};

const TEST_PROJECT_ID: &str = "tmux-agent-project";
/// What the stub `create-chat` mints for cursor.
const MINTED_CHAT_ID: &str = "chat-0001";
/// What the warp stub prints on its way out; the token is `tok-123`.
const WARP_BANNER: &str = "resume with: warp --resume tok-123";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A harness session type, the flag its resume argv carries, and the value after it.
struct Harness {
    session_type: &'static str,
    resume_flag: &'static str,
    resume_value: Option<&'static str>,
}

const HARNESSES: [Harness; 4] = [
    Harness {
        session_type: "cursor-cli",
        resume_flag: "--resume",
        resume_value: Some(MINTED_CHAT_ID),
    },
    Harness {
        session_type: "opencode-cli",
        resume_flag: "--continue",
        resume_value: None,
    },
    Harness {
        session_type: "muse-cli",
        resume_flag: "--continue",
        resume_value: None,
    },
    Harness {
        session_type: "warp-cli",
        resume_flag: "--resume",
        resume_value: Some("tok-123"),
    },
];

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A daemon whose harnesses are stubs that record how they were launched.
struct HarnessTmuxDaemon {
    host: Arc<DaemonSessionHost>,
    manager: Arc<ClaudeCliSessionManager>,
    config: DaemonConfig,
    sessions_base: PathBuf,
    launches: PathBuf,
    users: PathBuf,
    tmux: PrivateTmux,
    _dirs: Vec<tempfile::TempDir>,
    _repo: tempfile::TempDir,
}

/// `exits_with_banner`: the stub prints [`WARP_BANNER`] and exits, instead of holding its pane.
async fn a_daemon_with_a_private_tmux(exits_with_banner: bool) -> HarnessTmuxDaemon {
    let tmux = PrivateTmux::started();
    let sessions_dir = tempfile::tempdir().unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let launches = stub_dir.path().join("launches.log");
    let users = stub_dir.path().join("users.log");
    let stub = a_stub_harness(stub_dir.path(), &launches, &users, exits_with_banner);
    let repo = a_registered_project(sessions_dir.path());
    let (config_dir, config) = a_config_running(&stub);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config.clone(),
        sessions_dir.path().to_path_buf(),
        Arc::clone(&manager),
    );
    HarnessTmuxDaemon {
        host,
        manager,
        config,
        sessions_base: sessions_dir.path().to_path_buf(),
        launches,
        users,
        tmux,
        _dirs: vec![sessions_dir, stub_dir, config_dir],
        _repo: repo,
    }
}

/// One stub for every harness: answers `create-chat`, records its argv and the user it runs as,
/// then prints the warp banner and exits, or holds its pane.
fn a_stub_harness(dir: &Path, launches: &Path, users: &Path, exits_with_banner: bool) -> PathBuf {
    let script = a_stub_agent_script(dir, "stub_harness.sh")
        .with_prelude(&format!(
            "if [ \"$1\" = \"create-chat\" ]; then echo {MINTED_CHAT_ID}; exit 0; fi"
        ))
        .appending_argv_to(launches)
        .then_running(&format!("id -un >> \"{}\"", users.display()));
    if exits_with_banner {
        script
            .then_running(&format!("echo '{WARP_BANNER}'"))
            .build()
    } else {
        script
            .then_running(&format!("echo '{WARP_BANNER}'"))
            .then_reading_stdin()
            .build()
    }
}

fn a_config_running(stub: &Path) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let harnesses: String = ["opencode-cli", "muse-cli", "warp-cli"]
        .iter()
        .map(|t| format!("  {t}:\n    binary_path: {}\n", stub.display()))
        .collect();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\ncursor_cli:\n  binary_path: {}\ncli_harnesses:\n{harnesses}",
        stub.display()
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

/// The first stream of `session_id`'s main terminal from `host` — what re-attaches it lazily.
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

fn a_start_request(session_type: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: session_type.to_string(),
        model: "test-model".to_string(),
        run_in_tmux: true,
        ..Default::default()
    }
}

impl HarnessTmuxDaemon {
    async fn starts_in_tmux(&self, session_type: &str) -> String {
        self.host
            .start_session(Request::direct(a_start_request(session_type)))
            .await
            .unwrap_or_else(|e| panic!("StartSession {session_type} with run_in_tmux: {e}"))
            .into_inner()
            .session_id
    }

    fn metadata_of(&self, session_id: &str) -> tddy_core::SessionMetadata {
        let dir = self.sessions_base.join("sessions").join(session_id);
        tddy_core::read_session_metadata(&dir).expect("the session's .session.yaml is readable")
    }

    fn tmux_session_of(&self, session_id: &str) -> String {
        self.metadata_of(session_id)
            .tmux_session
            .expect("a tmux-run session records its tmux session")
    }

    fn launch_count(&self) -> usize {
        std::fs::read_to_string(&self.launches)
            .map(|log| log.lines().count())
            .unwrap_or(0)
    }

    /// A daemon restarted over the same sessions directory: no terminals registered, same tmux.
    fn restarted(&self) -> Arc<DaemonSessionHost> {
        self.restarted_with_its_manager().0
    }

    /// [`Self::restarted`], with the restarted daemon's terminal manager.
    fn restarted_with_its_manager(
        &self,
    ) -> (Arc<DaemonSessionHost>, Arc<ClaudeCliSessionManager>) {
        let manager = Arc::new(ClaudeCliSessionManager::new());
        let host = a_session_host(
            self.config.clone(),
            self.sessions_base.clone(),
            Arc::clone(&manager),
        );
        (host, manager)
    }

    async fn until_launched_in(&self, name: &str) {
        eventually("the stub to launch in its tmux session", OBSERVE, || {
            read_last_appended_argv(&self.launches)?;
            self.tmux
                .has_session(name)
                .then_some(())
                .ok_or_else(|| format!("tmux has no session {name:?} yet"))
        })
        .await;
    }

    async fn until_the_main_left_the_registry(&self, session_id: &str) {
        Self::until_the_main_left(&self.manager, session_id).await;
    }

    async fn until_the_main_left(manager: &ClaudeCliSessionManager, session_id: &str) {
        eventually_awaiting(
            "the main terminal to leave the registry",
            OBSERVE,
            || async {
                match manager.get(session_id).await {
                    None => Ok(()),
                    Some(_) => Err("the main terminal is still registered".to_string()),
                }
            },
        )
        .await;
    }

    async fn until_the_resume_token_is(&self, session_id: &str, token: &str) {
        eventually("the resume token to be recorded", OBSERVE, || {
            match self.metadata_of(session_id).harness_resume_token {
                Some(recorded) if recorded == token => Ok(()),
                other => Err(format!("recorded token: {other:?}")),
            }
        })
        .await;
    }

    fn resume_request(session_id: &str) -> ResumeSessionRequest {
        ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn each_cli_harness_starts_in_tmux_as_the_session_os_user() {
    for harness in &HARNESSES {
        // Given a daemon whose harnesses are stubs
        let daemon = a_daemon_with_a_private_tmux(false).await;

        // When a session of that type is started with run_in_tmux
        let session_id = daemon.starts_in_tmux(harness.session_type).await;

        // Then the harness runs in a tmux session, as the session's OS user, with a client attached
        let name = daemon.tmux_session_of(&session_id);
        daemon.until_launched_in(&name).await;
        until_attached(&daemon.tmux, &name, 1).await;
        let user = eventually("the stub to record its user", OBSERVE, || {
            std::fs::read_to_string(&daemon.users)
                .map(|s| s.trim().to_string())
                .map_err(|e| e.to_string())
                .and_then(|s| (!s.is_empty()).then_some(s).ok_or("empty".to_string()))
        })
        .await;
        assert_eq!(user, this_os_user(), "{}", harness.session_type);
        assert!(
            daemon.metadata_of(&session_id).pid.is_some(),
            "{}: the pane's pid is recorded",
            harness.session_type
        );
    }
}

#[tokio::test]
#[serial_test::serial]
async fn each_cli_harness_resumes_in_a_fresh_tmux_session_with_its_resume_argv() {
    for harness in &HARNESSES {
        // Given a tmux-run session whose daemon is gone and whose tmux session was killed
        let daemon = a_daemon_with_a_private_tmux(false).await;
        let session_id = daemon.starts_in_tmux(harness.session_type).await;
        let name = daemon.tmux_session_of(&session_id);
        daemon.until_launched_in(&name).await;
        daemon.host.shut_down_children().await;
        assert!(daemon
            .tmux
            .tmux(&["kill-session", "-t", &name])
            .status
            .success());
        record_the_warp_token_the_first_run_printed(&daemon, &session_id, harness);
        let fresh = daemon.restarted();

        // When it is resumed
        fresh
            .resume_session(Request::direct(HarnessTmuxDaemon::resume_request(
                &session_id,
            )))
            .await
            .expect("ResumeSession");

        // Then the harness is launched again, with its resume argv, in a tmux session with a client
        let resumed = daemon.tmux_session_of(&session_id);
        until_attached(&daemon.tmux, &resumed, 1).await;
        let argv = eventually("the harness to be relaunched", OBSERVE, || {
            let argv = read_last_appended_argv(&daemon.launches)?;
            argv.contains(&harness.resume_flag.to_string())
                .then_some(argv.clone())
                .ok_or_else(|| format!("last launch: {argv:?}"))
        })
        .await;
        let after_flag = argv
            .iter()
            .position(|a| a == harness.resume_flag)
            .and_then(|i| argv.get(i + 1))
            .map(String::as_str);
        assert_eq!(
            after_flag.filter(|_| harness.resume_value.is_some()),
            harness.resume_value
        );
        assert_eq!(daemon.launch_count(), 2, "{}", harness.session_type);
    }
}

/// Warp's token is printed only as the first run exits; this stands in for that exit so the
/// resume has a token to continue.
fn record_the_warp_token_the_first_run_printed(
    daemon: &HarnessTmuxDaemon,
    session_id: &str,
    harness: &Harness,
) {
    if harness.session_type != "warp-cli" {
        return;
    }
    let dir = daemon.sessions_base.join("sessions").join(session_id);
    let mut meta = daemon.metadata_of(session_id);
    meta.harness_resume_token = harness.resume_value.map(str::to_string);
    tddy_core::write_session_metadata(&dir, &meta).unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn a_live_tmux_run_harness_session_is_reattached_on_resume() {
    for harness in &HARNESSES {
        // Given a tmux-run session that outlived its daemon
        let daemon = a_daemon_with_a_private_tmux(false).await;
        let session_id = daemon.starts_in_tmux(harness.session_type).await;
        let name = daemon.tmux_session_of(&session_id);
        daemon.until_launched_in(&name).await;
        let agent_pid = daemon.metadata_of(&session_id).pid;
        daemon.host.shut_down_children().await;
        let fresh = daemon.restarted();

        // When it is resumed
        fresh
            .resume_session(Request::direct(HarnessTmuxDaemon::resume_request(
                &session_id,
            )))
            .await
            .expect("ResumeSession");

        // Then the same tmux session has a client again and the harness was not launched twice
        until_attached(&daemon.tmux, &name, 1).await;
        assert_eq!(daemon.tmux_session_of(&session_id), name);
        assert_eq!(daemon.launch_count(), 1, "{}", harness.session_type);
        assert_eq!(daemon.metadata_of(&session_id).pid, agent_pid);
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_warp_session_in_tmux_records_its_resume_token_from_the_final_screen() {
    // Given a warp session in tmux whose agent prints its resume banner and exits
    let daemon = a_daemon_with_a_private_tmux(true).await;
    let session_id = daemon.starts_in_tmux("warp-cli").await;

    // When the agent exits (its pane stays, dead, holding the banner)
    daemon.until_the_main_left_the_registry(&session_id).await;
    daemon.manager.exit_hook_settled(&session_id).await;

    // Then the token was read from the pane's final screen and recorded
    assert_eq!(
        daemon
            .metadata_of(&session_id)
            .harness_resume_token
            .as_deref(),
        Some("tok-123")
    );
    // And the tmux session is ended with the hook
    let name = daemon.tmux_session_of(&session_id);
    eventually("the tmux session to be killed", OBSERVE, || {
        (!daemon.tmux.has_session(&name))
            .then_some(())
            .ok_or_else(|| format!("tmux still has {name:?}"))
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_client_exit_does_not_run_a_harness_exit_hook() {
    // Given a warp session in tmux whose agent has printed its banner and still runs
    let daemon = a_daemon_with_a_private_tmux(false).await;
    let session_id = daemon.starts_in_tmux("warp-cli").await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.until_launched_in(&name).await;
    until_attached(&daemon.tmux, &name, 1).await;

    // When only the tmux client exits (a detach)
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    daemon.until_the_main_left_the_registry(&session_id).await;
    daemon.manager.exit_hook_settled(&session_id).await;

    // Then no token was recorded from the banner, and the agent's tmux session is alive
    assert_eq!(daemon.metadata_of(&session_id).harness_resume_token, None);
    assert!(daemon.tmux.has_session(&name));
}

#[tokio::test]
#[serial_test::serial]
async fn a_fresh_daemon_reattaches_a_live_harness_session_on_first_stream_without_restarting_it() {
    for session_type in ["opencode-cli", "warp-cli"] {
        // Given a tmux-run harness session that outlived its daemon
        let daemon = a_daemon_with_a_private_tmux(false).await;
        let session_id = daemon.starts_in_tmux(session_type).await;
        let name = daemon.tmux_session_of(&session_id);
        daemon.until_launched_in(&name).await;
        let agent_pid = daemon.metadata_of(&session_id).pid;
        daemon.host.shut_down_children().await;
        until_attached(&daemon.tmux, &name, 0).await;
        let (fresh, fresh_manager) = daemon.restarted_with_its_manager();

        // When a client first streams the session's main terminal from the fresh daemon
        let streamed = first_stream_of_the_main_terminal(&fresh, &session_id).await;

        // Then a client is attached to the same tmux session and the harness was not relaunched
        assert!(
            streamed.is_ok(),
            "{session_type}: the first stream re-attaches the main terminal: {streamed:?}"
        );
        assert!(
            fresh
                .terminal_sessions()
                .get_terminal(&session_id, MAIN_TERMINAL_ID)
                .await
                .is_some(),
            "{session_type}: the main terminal is registered"
        );
        let main = fresh_manager
            .get(&session_id)
            .await
            .expect("the re-attached main is in the registry");
        assert_eq!(main.kind, session_type, "the main keeps its session's kind");
        until_attached(&daemon.tmux, &name, 1).await;
        assert_eq!(daemon.launch_count(), 1, "{session_type} was not relaunched");
        assert_eq!(daemon.metadata_of(&session_id).pid, agent_pid, "{session_type}");
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_warp_agent_that_exits_while_detached_after_a_restart_records_its_resume_token() {
    // Given a warp session in tmux that outlived its daemon, re-attached by a fresh daemon whose
    // client then detached — no client is attached while the agent runs
    let daemon = a_daemon_with_a_private_tmux(false).await;
    let session_id = daemon.starts_in_tmux("warp-cli").await;
    let name = daemon.tmux_session_of(&session_id);
    daemon.until_launched_in(&name).await;
    daemon.host.shut_down_children().await;
    until_attached(&daemon.tmux, &name, 0).await;
    let (fresh, fresh_manager) = daemon.restarted_with_its_manager();
    first_stream_of_the_main_terminal(&fresh, &session_id)
        .await
        .expect("the first stream re-attaches the main terminal");
    until_attached(&daemon.tmux, &name, 1).await;
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    HarnessTmuxDaemon::until_the_main_left(&fresh_manager, &session_id).await;
    assert_eq!(daemon.metadata_of(&session_id).harness_resume_token, None);

    // When warp exits (end of input), its banner on the final screen
    let pane = format!("={name}:");
    assert!(daemon
        .tmux
        .tmux(&["send-keys", "-t", &pane, "C-d"])
        .status
        .success());

    // Then the fresh daemon read the token from the final screen and recorded it
    daemon.until_the_resume_token_is(&session_id, "tok-123").await;
    // And ended the tmux session with the hook
    eventually("the tmux session to be killed", OBSERVE, || {
        (!daemon.tmux.has_session(&name))
            .then_some(())
            .ok_or_else(|| format!("tmux still has {name:?}"))
    })
    .await;
}
