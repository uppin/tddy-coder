//! Acceptance: an agent tab of a target session runs **on the target** — the agent found by its bare
//! command on the target login's own `PATH`, in the session's worktree there, in a conversation
//! settled on the target — as a local tab terminal that is not the session's main. The daemon's host
//! needs no copy of the agent, and nothing handed to the target names a path of this host
//! (`#session-liveness` 8/8).
//!
//! The target is the shared stand-in ssh machine (`tddy_testing_commons::remote_target`): a stand-in
//! `ssh` that runs the remote command on this machine under a login shell of the target's own, whose
//! `PATH` starts with the machine's `opt/agents`. Each agent installed there records the directory
//! it started in and its arguments, then stays up on its terminal. The daemon's coding agents are
//! the built-in specs, over a host that holds either a stand-in of the agent or none at all.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::remote_agent_launch::{launch_agent_tab_on_target, TargetTab};
use tddy_agent_launch::restore_tabs::{new_target_agent_tab, NewTargetAgentTab};
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_agent_launch::tab_target_shell::TabTarget;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_remote_shell::{
    ExecOutput, ExecRequest, FileMode, InteractiveCommand, RemoteShell, RemoteShellError,
    ShellAddress, SshShell, StreamedExec,
};
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};
use tddy_testing_commons::wait::eventually;

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e70";
/// The id the tab is opened under.
const TAB_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e71";
/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
/// The chat Cursor's `create-chat` prints on the target.
const CHAT_MINTED_ON_THE_TARGET: &str = "chat-minted-on-the-target";

/// A safety net for a stand-in to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// The stand-in ssh machine sets the process's `PATH`: its tests take turns.
static ONE_SSH_MACHINE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------------------------------------------------------------------------
// The daemon's host
// ---------------------------------------------------------------------------

/// A host whose environment is exactly `env` and whose `PATH` is empty: an agent resolves on it
/// only through its spec's binary variable.
struct AHostOf {
    env: BTreeMap<String, String>,
}

impl ToolHost for AHostOf {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        tddy_actions::tool::ProcessToolHost.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// The built-in coding agents, with no `<tddyhome>/tools` overrides.
fn builtin_registry() -> tddy_actions::tool::ToolRegistry {
    tddy_coding_agent_specs::load_agent_registry(Path::new("/nonexistent/tools"))
        .expect("the built-in coding agents load")
}

/// The built-in coding agents on `host`.
fn the_builtin_agents_on(host: AHostOf) -> HostCodingAgents {
    HostCodingAgents::new(builtin_registry(), Arc::new(host))
}

/// A stand-in of an agent installed on the daemon's host, at a path of this host, with the
/// `tddy-tools` its closure needs beside it: it records that it ran, and how, and exits.
struct AnAgentOnThisHost {
    binary: PathBuf,
    tddy_tools: PathBuf,
    ran: PathBuf,
    _dir: tempfile::TempDir,
}

impl AnAgentOnThisHost {
    fn named(command: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the host's agent");
        let root = std::fs::canonicalize(dir.path()).expect("the host's directory resolves");
        let binary = root.join(command);
        let ran = root.join(format!("{command}-ran-on-this-host"));
        write_executable(
            &binary,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\necho 'host-chat'\n",
                ran.display()
            ),
        );
        let tddy_tools = root.join("tddy-tools");
        write_executable(&tddy_tools, "#!/bin/sh\nexit 0\n");
        Self {
            binary,
            tddy_tools,
            ran,
            _dir: dir,
        }
    }

    /// The directory the host's agent is installed in.
    fn dir(&self) -> PathBuf {
        self.binary
            .parent()
            .expect("the agent is in a directory")
            .to_path_buf()
    }

    /// Whether the host's copy of the agent was ever run.
    fn ever_ran(&self) -> bool {
        self.ran.exists()
    }
}

/// The variable the built-in spec of tool `id` names its binary by.
fn binary_variable_of(id: &str) -> String {
    builtin_registry()
        .get(id)
        .and_then(|def| def.binary.env.clone())
        .unwrap_or_else(|| panic!("{id}'s spec names its binary's variable"))
}

/// A daemon whose host holds `agent` as the binary of the coding agent `id`, and its `tddy-tools`:
/// the agent resolves on this host.
fn a_host_holding(id: &str, agent: &AnAgentOnThisHost) -> HostCodingAgents {
    the_builtin_agents_on(AHostOf {
        env: BTreeMap::from([
            (
                binary_variable_of(id),
                agent.binary.to_string_lossy().into_owned(),
            ),
            (
                binary_variable_of("tddy-tools"),
                agent.tddy_tools.to_string_lossy().into_owned(),
            ),
        ]),
    })
}

/// A daemon whose host holds no coding agent at all.
fn a_host_without_any_agent() -> HostCodingAgents {
    the_builtin_agents_on(AHostOf {
        env: BTreeMap::new(),
    })
}

// ---------------------------------------------------------------------------
// The target
// ---------------------------------------------------------------------------

/// The stand-in ssh machine with the session's worktree on it.
struct ATarget {
    machine: StandInSshTarget,
    worktree: PathBuf,
}

fn a_target_holding_the_worktree() -> ATarget {
    let machine = StandInSshTarget::behind_ssh();
    let worktree = machine.worktree_of(SESSION_ID);
    std::fs::create_dir_all(&worktree).expect("the session's worktree on the target");
    ATarget { machine, worktree }
}

impl ATarget {
    /// Install the agent `command` on the target, first on its login `PATH`: run as `create-chat`
    /// it prints [`CHAT_MINTED_ON_THE_TARGET`] (Cursor's mint); otherwise it records the directory
    /// it started in and its arguments, and stays up on its terminal.
    fn installs_the_agent(&self, command: &str) -> &Self {
        write_executable(
            &self.machine.agents_dir().join(command),
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = create-chat ]; then echo '{CHAT_MINTED_ON_THE_TARGET}'; exit 0; fi\n\
                 {{ /bin/pwd -P; printf '%s\\n' \"$@\"; }} > '{record}.part'\n\
                 /bin/mv '{record}.part' '{record}'\n\
                 exec /bin/sh\n",
                record = self.run_record(command).display()
            ),
        );
        self
    }

    fn run_record(&self, command: &str) -> PathBuf {
        self.machine
            .root
            .join(format!("{command}-ran-on-the-target"))
    }

    /// The directory the target's agent `command` started in, and the arguments it was given, once
    /// it has started.
    async fn agent_run(&self, command: &str) -> (PathBuf, Vec<String>) {
        let record = self.run_record(command);
        eventually("the target's agent to start", OBSERVE, || {
            let recorded = std::fs::read_to_string(&record)
                .map_err(|_| "the target's agent has not started yet".to_string())?;
            let mut lines = recorded.lines();
            let cwd = lines
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "the target's agent recorded nothing yet".to_string())?;
            Ok((cwd, lines.map(str::to_string).collect()))
        })
        .await
    }

    /// The remote shell that reaches the target, recording what it is handed.
    fn the_recording_shell(&self) -> Arc<ARecordingShell> {
        Arc::new(ARecordingShell {
            inner: SshShell::through(self.machine.root.join("bin/ssh"), TARGET_ALIAS),
            interactive: StdMutex::new(Vec::new()),
            written: StdMutex::new(Vec::new()),
        })
    }
}

/// The session's remote shell, recording every command a terminal is opened on and every file
/// written on the target, and passing each on to the stand-in `ssh`.
struct ARecordingShell {
    inner: SshShell,
    interactive: StdMutex<Vec<String>>,
    written: StdMutex<Vec<String>>,
}

impl ARecordingShell {
    fn interactive_commands(&self) -> Vec<String> {
        self.interactive.lock().unwrap().clone()
    }

    fn written_paths(&self) -> Vec<String> {
        self.written.lock().unwrap().clone()
    }
}

#[async_trait]
impl RemoteShell for ARecordingShell {
    fn address(&self) -> ShellAddress {
        self.inner.address()
    }

    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        self.inner.exec(request).await
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        self.inner.exec_streamed(command).await
    }

    fn interactive(&self, command: &str) -> InteractiveCommand {
        self.interactive.lock().unwrap().push(command.to_string());
        self.inner.interactive(command)
    }

    async fn write_file(
        &self,
        path: &str,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), RemoteShellError> {
        self.written.lock().unwrap().push(path.to_string());
        self.inner.write_file(path, bytes, mode).await
    }
}

// ---------------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------------

/// A daemon holding one session's terminals, and the session's directory on this host.
struct ADaemon {
    manager: Arc<CliSessionManager>,
    coding_agents: HostCodingAgents,
    session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

fn a_daemon_with(coding_agents: HostCodingAgents) -> ADaemon {
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        coding_agents,
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}

/// An agent tab opened on a target: what was settled for it, and its terminal.
struct AnOpenedTab {
    settled: NewTargetAgentTab,
    terminal: Arc<PtyHandle>,
}

impl ADaemon {
    /// Open the session's agent tab [`TAB_ID`] of `harness` on the target `shell` reaches, in
    /// `worktree` there: its conversation settled on the target, then its terminal launched there.
    async fn opens_an_agent_tab_on(
        &self,
        harness: &str,
        shell: Arc<dyn RemoteShell>,
        worktree: &Path,
    ) -> Result<AnOpenedTab, String> {
        let target = TabTarget {
            shell: Arc::clone(&shell),
            worktree: worktree.to_path_buf(),
        };
        let settled = new_target_agent_tab(&self.coding_agents, harness, &target)
            .await
            .map_err(|status| status.message().to_string())?;
        let terminal = launch_agent_tab_on_target(
            &self.manager,
            SESSION_ID,
            TAB_ID,
            TargetTab {
                shell,
                worktree,
                session_dir: &self.session_dir,
            },
            &settled.agent,
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok(AnOpenedTab { settled, terminal })
    }
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_tab_opened_on_target_runs_the_agent_in_the_target_worktree() {
    // Given a target session whose machine has Claude Code installed, and a daemon host that has its
    // own copy too
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_holding_the_worktree();
    target.installs_the_agent("claude");
    let claude_on_this_host = AnAgentOnThisHost::named("claude");
    let daemon = a_daemon_with(a_host_holding("claude", &claude_on_this_host));
    assert!(
        daemon.coding_agents.resolve("claude", None).is_ok(),
        "the daemon's host must resolve its own Claude Code for this test"
    );

    // When a Claude tab is opened on the target
    let tab = daemon
        .opens_an_agent_tab_on("claude-cli", target.the_recording_shell(), &target.worktree)
        .await
        .expect("the agent tab starts on the target");

    // Then the target's Claude runs in the session's worktree there, starting the tab's own
    // conversation ...
    assert_eq!(
        target.agent_run("claude").await,
        (
            target.worktree.clone(),
            vec![
                "--session-id".to_string(),
                tab.settled.conversation_id.clone(),
                "--permission-mode".to_string(),
                "auto".to_string(),
            ]
        )
    );
    // ... the host's copy never ran, and the tab is an agent tab of the session under the id it was
    // opened with — not its main, and recording no agent pid on the target as a main does
    assert!(!claude_on_this_host.ever_ran(), "the host's Claude ran");
    assert_eq!(
        (
            tab.terminal.terminal_id.as_str(),
            tab.terminal.kind.as_str()
        ),
        (TAB_ID, "claude-cli")
    );
    assert!(
        daemon.manager.get(SESSION_ID).await.is_none(),
        "a tab is never the session's main"
    );
    assert!(
        !target.worktree.join(".tddy/agents").exists(),
        "a tab records no agent pid on the target"
    );
}

#[tokio::test]
async fn a_target_agent_tab_starts_when_the_daemon_host_has_no_agent() {
    // Given a target whose login `PATH` has Claude Code, and a daemon host with no coding agent at
    // all
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_holding_the_worktree();
    target.installs_the_agent("claude");
    let daemon = a_daemon_with(a_host_without_any_agent());
    assert!(
        daemon.coding_agents.resolve("claude", None).is_err(),
        "the daemon's host must not resolve Claude Code for this test"
    );

    // When a Claude tab is opened on the target
    let opened = daemon
        .opens_an_agent_tab_on("claude-cli", target.the_recording_shell(), &target.worktree)
        .await;

    // Then it starts, and the target's own Claude runs in the session's worktree there
    let tab = opened.expect("a tab on the target needs no agent on the daemon's host");
    assert_eq!(
        target.agent_run("claude").await.0,
        target.worktree.clone(),
        "the target's Claude ran elsewhere"
    );
    assert_eq!(
        tab.settled.agent.argv.first().map(String::as_str),
        Some("claude")
    );
}

#[tokio::test]
async fn a_target_agent_tabs_chat_is_minted_on_the_target() {
    // Given a target whose machine has Cursor's agent installed, and a daemon host whose own copy
    // would mint a different chat
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_holding_the_worktree();
    target.installs_the_agent("agent");
    let cursor_on_this_host = AnAgentOnThisHost::named("agent");
    let daemon = a_daemon_with(a_host_holding("cursor", &cursor_on_this_host));

    // When a Cursor tab is opened on the target
    let tab = daemon
        .opens_an_agent_tab_on("cursor-cli", target.the_recording_shell(), &target.worktree)
        .await
        .expect("the agent tab starts on the target");

    // Then its conversation is the chat the target minted, the host's agent was never run, and the
    // target's Cursor resumes that chat in the session's worktree there
    assert_eq!(tab.settled.conversation_id, CHAT_MINTED_ON_THE_TARGET);
    assert!(
        !cursor_on_this_host.ever_ran(),
        "the host's Cursor minted a chat"
    );
    let (cwd, args) = target.agent_run("agent").await;
    assert_eq!(
        (cwd, args[..2].to_vec()),
        (
            target.worktree.clone(),
            vec![
                "--resume".to_string(),
                CHAT_MINTED_ON_THE_TARGET.to_string()
            ]
        )
    );
}

#[tokio::test]
async fn a_target_agent_tab_hands_the_target_no_host_path() {
    // Given a target whose machine has Claude Code installed, and a daemon host holding its own copy
    // at a path of this host
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_holding_the_worktree();
    target.installs_the_agent("claude");
    let claude_on_this_host = AnAgentOnThisHost::named("claude");
    let daemon = a_daemon_with(a_host_holding("claude", &claude_on_this_host));
    let shell = target.the_recording_shell();

    // When a Claude tab is opened on the target, and its agent has started
    daemon
        .opens_an_agent_tab_on(
            "claude-cli",
            Arc::clone(&shell) as Arc<dyn RemoteShell>,
            &target.worktree,
        )
        .await
        .expect("the agent tab starts on the target");
    target.agent_run("claude").await;

    // Then the one command the tab's terminal carries enters the worktree on the target and names
    // neither the session's directory on this host nor the host's agent ...
    let commands = shell.interactive_commands();
    let host_paths = [
        daemon.session_dir.display().to_string(),
        claude_on_this_host.dir().display().to_string(),
    ];
    let the_command = commands.first().cloned().unwrap_or_default();
    assert_eq!(
        (
            commands.len(),
            the_command.contains(&format!("cd '{}'", target.worktree.display())),
            host_paths
                .iter()
                .any(|path| the_command.contains(path.as_str())),
        ),
        (1, true, false),
        "the command carried to the target: {the_command}"
    );
    // ... and every file the tab wrote there is in the worktree's `.tddy/`, never a host path
    let on_the_target = target.worktree.join(".tddy").display().to_string();
    let elsewhere: Vec<String> = shell
        .written_paths()
        .into_iter()
        .filter(|path| !path.starts_with(&on_the_target))
        .collect();
    assert_eq!(elsewhere, Vec::<String>::new());
}
