//! A daemon whose `claude` is a stub run in a private tmux — the fixture of the suites that start a
//! tmux-run claude-cli session through `StartSession` and then act on it (`claude_cli_tmux_session_acceptance`,
//! `delete_session`). Moved out of `claude_cli_tmux_session_acceptance.rs` so the delete suite drives
//! the same session (`#tab-hardening` 2/4).
//!
//! Needs the suite to declare `mod support;` beside it: the private tmux and the token are there.
//! Each suite uses a subset, so unused-item warnings are allowed for the module.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::process_start::ProcessStart;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    DeleteSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_last_appended_argv};
use tddy_testing_commons::wait::eventually;

use crate::support::{this_os_user, PrivateTmux, OBSERVE, VALID_TOKEN};

pub const TEST_MODEL: &str = "claude-opus-4-8";
pub const TEST_PROJECT_ID: &str = "tmux-agent-project";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A daemon whose `claude` is a stub that records what it was launched with, then holds its pane.
pub struct TmuxAgentDaemon {
    pub host: Arc<DaemonSessionHost>,
    pub manager: Arc<CliSessionManager>,
    pub sessions_base: PathBuf,
    pub launches: PathBuf,
    pub environment: PathBuf,
    pub working_directory: PathBuf,
    pub tmux: PrivateTmux,
    _dirs: Vec<tempfile::TempDir>,
    _repo: tempfile::TempDir,
}

pub async fn a_daemon_with_a_private_tmux() -> TmuxAgentDaemon {
    a_daemon_whose_agent(AgentTemperament::HoldsItsPane).await
}

/// How the stub `claude` behaves when its tmux session is killed (tmux sends SIGHUP to the pane).
pub enum AgentTemperament {
    /// Reads its pane until it closes.
    HoldsItsPane,
    /// Ignores SIGHUP and never reads its pane, so nothing a tmux kill does ends it: only a
    /// signal sent to its own pid does.
    IgnoresHangupAndNeverReadsItsPane,
}

pub async fn a_daemon_whose_agent(temperament: AgentTemperament) -> TmuxAgentDaemon {
    let tmux = PrivateTmux::started();
    let sessions_dir = tempfile::tempdir().unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let launches = stub_dir.path().join("launches.log");
    let environment = stub_dir.path().join("environment.txt");
    let working_directory = stub_dir.path().join("cwd.txt");
    let mut stub = a_stub_agent_script(stub_dir.path(), "stub_claude.sh");
    if !matches!(temperament, AgentTemperament::HoldsItsPane) {
        // An ignored signal stays ignored across `exec`, so what the stub ends in ignores it too.
        stub = stub.with_prelude("trap '' HUP");
    }
    let stub = stub
        .with_prelude(&format!("pwd -P > \"{}\"", working_directory.display()))
        .recording_env_to(&environment, &["CLAUDE_CODE_NO_FLICKER"])
        .appending_argv_to(&launches);
    let stub = match temperament {
        AgentTemperament::IgnoresHangupAndNeverReadsItsPane => {
            // The pane's terminal stays its stdin/stdout/stderr: a process that closed every slave
            // end would make tmux see its pty close on Linux and destroy the session at once,
            // before the test has a session to delete.
            stub.then_running("exec sleep 120")
        }
        AgentTemperament::HoldsItsPane => stub.then_reading_stdin(),
    }
    .build();
    let repo = a_registered_project(sessions_dir.path());
    let (config_dir, config) = a_config_running(&stub);
    let manager = Arc::new(CliSessionManager::new());
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

/// A daemon config, and the coding agents the host installs: `claude` is the stub.
type AConfigAndItsAgents = (
    DaemonConfig,
    Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
);

fn a_config_running(stub_agent: &Path) -> (tempfile::TempDir, AConfigAndItsAgents) {
    let dir = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let yaml = "allowed_tools:\n  - path: /bin/true\n    label: true\n";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        std::path::Path::new(&(path))
            .parent()
            .expect("a config directory")
            .join("users.yaml"),
        format!("- github_user: \"{user}\"\n  os_user: \"{user}\"\n"),
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    let agents =
        tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[("claude", stub_agent)]);
    (dir, (config, agents))
}

pub fn a_session_host(
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

pub fn a_claude_cli_start_request(run_in_tmux: bool, sandbox: bool) -> StartSessionRequest {
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
    pub async fn starts_a_session_in_tmux(&self) -> String {
        self.host
            .start_session(Request::direct(a_claude_cli_start_request(true, false)))
            .await
            .expect("StartSession with run_in_tmux must succeed")
            .into_inner()
            .session_id
    }

    /// A tmux-run session whose agent is running in its tmux session: its id and that tmux session.
    pub async fn a_running_tmux_session(&self) -> (String, String) {
        let session_id = self.starts_a_session_in_tmux().await;
        let name = self.tmux_session_of(&session_id);
        self.until_the_agent_ran_in_tmux_session(&self.tmux, &name)
            .await;
        (session_id, name)
    }

    pub async fn deletes(&self, session_id: &str) {
        self.host
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("DeleteSession");
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    pub fn metadata_of(&self, session_id: &str) -> tddy_core::SessionMetadata {
        tddy_core::read_session_metadata(&self.session_dir(session_id))
            .expect("the session's .session.yaml is readable")
    }

    /// The pid the session recorded for its agent: its pane's process.
    pub fn agent_pid_of(&self, session_id: &str) -> u32 {
        self.metadata_of(session_id)
            .pid
            .expect("a tmux-run session records its agent's pid")
    }

    /// The tmux node of `session_id`'s main in its tab roster (`tabs.yaml`).
    fn with_tmux_main_node<T>(
        &self,
        session_id: &str,
        change: impl FnOnce(&mut tddy_core::tab_roster::LaunchNode) -> T,
    ) -> T {
        let dir = self.session_dir(session_id);
        let mut roster =
            tddy_core::tab_roster::read_tab_roster(&dir).expect("the session's tabs.yaml reads");
        let node = roster
            .tabs
            .iter_mut()
            .find(|tab| tab.terminal_id == "main")
            .and_then(|main| {
                main.launch
                    .iter_mut()
                    .find(|node| node.kind == tddy_core::tab_roster::TMUX_KIND)
            })
            .expect("a tmux-run session's roster records its main's tmux node");
        let answer = change(node);
        tddy_core::tab_roster::write_tab_roster(&dir, &roster).expect("tabs.yaml is rewritten");
        answer
    }

    /// When the session's roster says its agent started: the tmux main node's start.
    pub fn recorded_agent_start(&self, session_id: &str) -> Option<ProcessStart> {
        self.with_tmux_main_node(session_id, |node| {
            node.property(tddy_core::tab_roster::START_PROPERTY)
                .map(|secs| ProcessStart::from_epoch_secs(secs.parse().expect("whole seconds")))
        })
    }

    /// Rewrite the start the session's roster recorded for its agent to `start`.
    pub fn records_the_agent_start(&self, session_id: &str, start: ProcessStart) {
        self.with_tmux_main_node(session_id, |node| {
            node.properties.insert(
                tddy_core::tab_roster::START_PROPERTY.to_string(),
                start.epoch_secs().to_string(),
            );
        });
    }

    /// The tmux session the daemon recorded for `session_id`, once it has recorded one.
    pub fn tmux_session_of(&self, session_id: &str) -> String {
        self.metadata_of(session_id)
            .tmux_session
            .expect("a tmux-run session records its tmux session")
    }

    /// The worktree the session's agent was cut into.
    pub fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("the session records its worktree"),
        )
        .canonicalize()
        .unwrap()
    }

    pub async fn until_the_agent_ran_in_tmux_session(&self, tmux: &PrivateTmux, name: &str) {
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

pub fn a_process_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Kills `pid` when dropped — so an agent a test expects to survive does not outlive the test,
/// whether its assertions pass or not.
pub struct KilledOnDrop(pub u32);

impl Drop for KilledOnDrop {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.0 as i32, libc::SIGKILL);
        }
    }
}
