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
//! Extended by `#tab-hardening` 1/4: before its terminal starts, an agent tab asks the target's login
//! shell for its agent, and a target without it fails the tab naming the agent; an unreachable
//! target fails it `UNAVAILABLE` (PRD-2026-11-21-tab-errors.md).
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tddy_test_hosts::a_tool_host;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_agent_launch::remote_agent_launch::{
    launch_agent_tab_on_target, tab_launch_status, RemoteLaunchError, TargetTab,
};
use tddy_agent_launch::restore_tabs::{new_target_agent_tab, NewTargetAgentTab};
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_agent_launch::tab_target_shell::TabTarget;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_remote_shell::{
    ExecOutput, ExecRequest, FileMode, InteractiveCommand, RemoteShell, RemoteShellError,
    ShellAddress, SshShell, StreamedExec,
};
use tddy_rpc::Code;
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};
use tddy_testing_commons::wait::eventually;

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e70";
/// The id the tab is opened under.
const TAB_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e71";
/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
/// The chat Cursor's `create-chat` prints on the target.
/// Cursor's chat ids are UUIDs: its persistent session refuses any other.
const CHAT_MINTED_ON_THE_TARGET: &str = "6f1c2a9e-3b7d-4e58-9a0c-1d2e3f4a5b6c";
/// What `create-chat` writes when the target runs it under a terminal, as `coder ssh` does: the
/// terminal queries Cursor's TUI library sends on start (OSC 11 background colour, DSR cursor
/// position), the chat id with the terminal's `\r\n`, then the show-cursor it sends on exit.
const CREATE_CHAT_UNDER_A_TERMINAL: &str = "\\033]11;?\\033\\\\\\033[6n%s\\r\\n\\033[?25h\\r\\n";

/// What OpenSSH says of a host it cannot resolve.
const SSH_COMPLAINT: &str =
    "ssh: Could not resolve hostname buildbox: nodename nor servname provided, or not known";

/// A safety net for a stand-in to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// The stand-in ssh machine sets the process's `PATH`: its tests take turns.
static ONE_SSH_MACHINE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------------------------------------------------------------------------
// The daemon's host
// ---------------------------------------------------------------------------

/// The built-in coding agents, with no `<tddyhome>/tools` overrides.
fn builtin_registry() -> tddy_actions::tool::ToolRegistry {
    tddy_coding_agent_specs::load_agent_registry(Path::new("/nonexistent/tools"))
        .expect("the built-in coding agents load")
}

/// The built-in coding agents on `host`.
fn the_builtin_agents_on(host: tddy_test_hosts::AToolHost) -> HostCodingAgents {
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
    the_builtin_agents_on(
        a_tool_host()
            .with_envs(BTreeMap::from([
                (
                    binary_variable_of(id),
                    agent.binary.to_string_lossy().into_owned(),
                ),
                (
                    binary_variable_of("tddy-tools"),
                    agent.tddy_tools.to_string_lossy().into_owned(),
                ),
            ]))
            .build(),
    )
}

/// A daemon whose host holds no coding agent at all.
fn a_host_without_any_agent() -> HostCodingAgents {
    the_builtin_agents_on(a_tool_host().build())
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

/// The stand-in ssh machine with the session's worktree on it, whose login `PATH` is its own agents
/// directory — empty — and the system's directories: no agent is installed there, whatever this
/// host has on its own `PATH`.
fn a_target_without_any_agent() -> ATarget {
    let machine = StandInSshTarget::behind_ssh_with_only_the_system_path();
    let worktree = machine.worktree_of(SESSION_ID);
    std::fs::create_dir_all(&worktree).expect("the session's worktree on the target");
    ATarget { machine, worktree }
}

/// A target ssh cannot reach: its `ssh` fails as OpenSSH does — [`SSH_COMPLAINT`] on stderr, status
/// 255 — and runs nothing.
struct AnUnreachableTarget {
    ssh: PathBuf,
    worktree: PathBuf,
    _dir: tempfile::TempDir,
}

fn an_unreachable_target() -> AnUnreachableTarget {
    let dir = tempfile::tempdir().expect("a directory for the stand-in ssh");
    let ssh = dir.path().join("ssh");
    write_executable(
        &ssh,
        &format!("#!/bin/sh\necho '{SSH_COMPLAINT}' >&2\nexit 255\n"),
    );
    AnUnreachableTarget {
        ssh,
        worktree: PathBuf::from("/home/dev/repo/.worktrees").join(SESSION_ID),
        _dir: dir,
    }
}

impl AnUnreachableTarget {
    /// The remote shell that tries to reach the target, recording what it is handed.
    fn the_recording_shell(&self) -> Arc<ARecordingShell> {
        Arc::new(ARecordingShell {
            inner: SshShell::through(&self.ssh, TARGET_ALIAS),
            interactive: StdMutex::new(Vec::new()),
            written: StdMutex::new(Vec::new()),
        })
    }
}

impl ATarget {
    /// Install the agent `command` on the target, first on its login `PATH`: run as `create-chat`
    /// it prints [`CHAT_MINTED_ON_THE_TARGET`] (Cursor's mint); otherwise it records the directory
    /// it started in and its arguments, and stays up on its terminal.
    fn installs_the_agent(&self, command: &str) -> &Self {
        self.installs_the_agent_printing_its_chat_as(command, "%s\\n")
    }

    /// [`Self::installs_the_agent`], its `create-chat` printing [`CHAT_MINTED_ON_THE_TARGET`]
    /// through the `printf` format `chat_format`.
    fn installs_the_agent_printing_its_chat_as(&self, command: &str, chat_format: &str) -> &Self {
        write_executable(
            &self.machine.agents_dir().join(command),
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = create-chat ]; then printf '{chat_format}' '{CHAT_MINTED_ON_THE_TARGET}'; exit 0; fi\n\
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

    /// Launch the session's agent tab [`TAB_ID`] of `harness` on the target `shell` reaches, in
    /// `worktree` there, its conversation settled first: the launch's own outcome.
    async fn launches_an_agent_tab_on(
        &self,
        harness: &str,
        shell: Arc<dyn RemoteShell>,
        worktree: &Path,
    ) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
        let target = TabTarget {
            shell: Arc::clone(&shell),
            worktree: worktree.to_path_buf(),
        };
        let settled = new_target_agent_tab(&self.coding_agents, harness, &target)
            .await
            .expect("the tab's conversation is settled without the target");
        launch_agent_tab_on_target(
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
    }

    /// Whether the tab [`TAB_ID`] is registered as a terminal of the session.
    async fn has_the_tab(&self) -> bool {
        self.manager
            .get_terminal(SESSION_ID, TAB_ID)
            .await
            .is_some()
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

/// Over `coder ssh` the target runs `create-chat` under a terminal, so its stdout carries terminal
/// escapes around the chat id and a show-cursor (`\e[?25h`) on a line of its own after it. The
/// daemon took that last line as the chat, recorded `"\e[?25h"`, and Cursor refused to start
/// ("Persistent-session chat ID must be a UUID") — on start and on every resume.
#[tokio::test]
async fn a_target_agent_tabs_chat_minted_under_a_terminal_is_the_bare_chat_id() {
    // Given a target whose Cursor prints its chat the way it does under `coder ssh`'s terminal
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_holding_the_worktree();
    target.installs_the_agent_printing_its_chat_as("agent", CREATE_CHAT_UNDER_A_TERMINAL);
    let daemon = a_daemon_with(a_host_without_any_agent());

    // When a Cursor tab is opened on the target
    let tab = daemon
        .opens_an_agent_tab_on("cursor-cli", target.the_recording_shell(), &target.worktree)
        .await
        .expect("the agent tab starts on the target");

    // Then its conversation is the chat id alone, and the target's Cursor resumes exactly that chat
    assert_eq!(tab.settled.conversation_id, CHAT_MINTED_ON_THE_TARGET);
    let (_, args) = target.agent_run("agent").await;
    assert_eq!(
        args[..2].to_vec(),
        vec![
            "--resume".to_string(),
            CHAT_MINTED_ON_THE_TARGET.to_string()
        ]
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

#[tokio::test]
async fn an_agent_tab_whose_agent_is_missing_on_the_target_fails_before_its_terminal_starts() {
    // Given a target whose login `PATH` holds no Claude Code, and a daemon host with none either
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = a_target_without_any_agent();
    let daemon = a_daemon_with(a_host_without_any_agent());
    let shell = target.the_recording_shell();

    // When a Claude tab is launched on the target
    let failure = daemon
        .launches_an_agent_tab_on(
            "claude-cli",
            Arc::clone(&shell) as Arc<dyn RemoteShell>,
            &target.worktree,
        )
        .await
        .err()
        .expect("a tab whose agent the target lacks fails");

    // Then it fails naming the agent the target lacks, no terminal carried a command to the target,
    // and no tab is registered
    assert_eq!(
        failure,
        RemoteLaunchError::AgentNotOnTarget {
            command: "claude".to_string()
        }
    );
    assert_eq!(shell.interactive_commands(), Vec::<String>::new());
    assert!(!daemon.has_the_tab().await, "no tab is registered");
}

#[tokio::test]
async fn an_agent_tab_on_an_unreachable_target_fails_unavailable() {
    // Given a target ssh cannot reach, and a daemon host with no coding agent
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let target = an_unreachable_target();
    let daemon = a_daemon_with(a_host_without_any_agent());
    let shell = target.the_recording_shell();

    // When a Claude tab is launched on the target
    let failure = daemon
        .launches_an_agent_tab_on(
            "claude-cli",
            Arc::clone(&shell) as Arc<dyn RemoteShell>,
            &target.worktree,
        )
        .await
        .err()
        .expect("a tab on an unreachable target fails");

    // Then it is reported UNAVAILABLE in what ssh said, no terminal carried a command to the
    // target, and no tab is registered
    let status = tab_launch_status(&failure);
    assert_eq!(
        (status.code(), status.message().contains(SSH_COMPLAINT)),
        (Code::Unavailable, true),
        "the status: {status:?}"
    );
    assert_eq!(shell.interactive_commands(), Vec::<String>::new());
    assert!(!daemon.has_the_tab().await, "no tab is registered");
}

/// A live Coder workspace, named by the environment: `CODER_URL`, `CODER_SESSION_TOKEN` (e.g. from
/// `coder login token`), `TDDY_LIVE_CODER_WORKSPACE` (`<owner>/<name>`) and
/// `TDDY_LIVE_CODER_WORKTREE` (a directory on it). Run with `--ignored`.
fn a_live_coder_workspace() -> (Arc<dyn RemoteShell>, PathBuf) {
    let var = |name: &str| {
        std::env::var(name).unwrap_or_else(|_| panic!("{name} names the live workspace"))
    };
    let workspace = var("TDDY_LIVE_CODER_WORKSPACE");
    let (owner, name) = workspace
        .split_once('/')
        .expect("TDDY_LIVE_CODER_WORKSPACE is <owner>/<name>");
    let shell = tddy_remote_shell::CoderShell::new(
        tddy_remote_shell::CoderCredentials::new(
            var("CODER_URL"),
            tddy_credentials::SecretString::new(var("CODER_SESSION_TOKEN")),
        ),
        tddy_remote_shell::CoderWorkspaceRef::new(owner, name),
    );
    (
        Arc::new(shell),
        PathBuf::from(var("TDDY_LIVE_CODER_WORKTREE")),
    )
}

/// The live counterpart of [`a_target_agent_tabs_chat_minted_under_a_terminal_is_the_bare_chat_id`]:
/// a real `coder ssh` runs `create-chat` under its terminal, and the chat the daemon settles on is
/// a bare UUID Cursor accepts.
#[tokio::test]
#[ignore = "needs a live Coder workspace with Cursor installed; see a_live_coder_workspace"]
async fn a_cursor_tab_on_a_live_coder_workspace_settles_a_uuid_chat() {
    // Given a live Coder workspace with Cursor on its login PATH, and a daemon host without it
    let (shell, worktree) = a_live_coder_workspace();
    let daemon = a_daemon_with(a_host_without_any_agent());

    // When a Cursor tab's conversation is settled on it
    let settled = new_target_agent_tab(
        &daemon.coding_agents,
        "cursor-cli",
        &TabTarget { shell, worktree },
    )
    .await
    .map_err(|status| status.message().to_string())
    .expect("the chat is minted on the workspace");

    // Then the chat is a UUID, with nothing of the terminal left in it
    assert!(
        uuid::Uuid::parse_str(&settled.conversation_id).is_ok(),
        "settled {:?}, which Cursor refuses",
        settled.conversation_id
    );
}
