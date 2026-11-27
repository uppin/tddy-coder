//! Launching a session's agent on its **target**: a machine the daemon reaches through the
//! remote-shell port ([`tddy_remote_shell::RemoteShell`]), with no tddy software on it.
//!
//! The session stays an ordinary local one whose PTY child is the transport's client. The local PTY
//! runs [`RemoteShell::interactive`] over [`remote_agent_command`], in the session's own directory;
//! the agent's whole environment travels inside that command, because a remote shell forwards none.
//! The command records the agent's pid on the target before it `exec`s the agent, so once the agent
//! has outlived its startup window the session pointer there can name the agent as its owner.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-ssh-agent-launch.md.

mod agent_probe;
mod session_files;
mod startup;
mod tab_launch_status;
mod target;
mod target_agent_tab;
mod target_tab;
mod target_tmux_location;
mod target_tmux_tab;

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_actions::tool::TemplateParams;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner};
use tddy_core::ssh_exec::shell_single_quote;
#[cfg(test)]
use tddy_core::SessionTarget;
use tddy_remote_shell::{InteractiveExit, RemoteShell, RemoteShellError};

use crate::cli_session_pointer::{
    record_remote_owner_from, session_pointer_path, PlacedSessionPointer, PointerHost,
    RemoteAgentPid,
};
use crate::remote_agent_launch::startup::settled_last_output;
use crate::remote_agent_launch::startup::wait_for_agent_start;
use crate::remote_agent_launch::startup::AgentStart;
use crate::remote_agent_launch::startup::{exits_during_startup, StartupJudgement};
use crate::spec_launch::{HostAgent, HostPtySession, SpecLaunchError};

/// Where the remote command records a session's agent's pid, relative to the session's worktree on
/// the target: what a caller that stops the agent there reads.
pub use crate::cli_session_pointer::remote_agent_pid_file;
pub use crate::remote_agent_launch::agent_probe::{probe_agent_on_target, probe_tmux_on_target};
use crate::remote_agent_launch::session_files::plan_the_launch;
use crate::remote_agent_launch::session_files::write_rendered_session_files;
use crate::remote_agent_launch::session_files::SessionFilesPlace;
pub use crate::remote_agent_launch::tab_launch_status::tab_launch_status;
pub(crate) use crate::remote_agent_launch::target::target_config_host;
pub use crate::remote_agent_launch::target::{
    proto_of_session_target, session_target_of_proto, shell_for_target, TargetError,
};
pub use crate::remote_agent_launch::target_agent_tab::{
    launch_agent_tab_on_target, TargetAgentTab,
};
pub use crate::remote_agent_launch::target_tab::{
    launch_tab_on_target, target_shell_tab_command, TargetTab,
};
pub use crate::remote_agent_launch::target_tmux_location::{tmux_location_of, TargetTmux};
pub use crate::remote_agent_launch::target_tmux_tab::{
    enable_target_tmux_clipboard, enable_target_tmux_clipboard_logged,
    end_target_tmux_session_named, launch_tmux_tab_on_target, list_target_tmux_sessions,
    target_tmux_attach_command, target_tmux_session_lives, target_tmux_tab_command,
};

/// The base set of the agent's environment on a target, expanded by the target's own login shell:
/// its `HOME`, `PATH` and `TMPDIR`, never the daemon host's.
const TARGET_BASE_ENV: &str = r#"HOME="$HOME" PATH="$PATH" TMPDIR="${TMPDIR:-/tmp}""#;

/// `argv` run through the target's login shell under `env -i`: the target expands its own base
/// set ([`TARGET_BASE_ENV`]), the agent is told the terminal it draws on
/// ([`tddy_pty::terminal_variables::TERMINAL_VARIABLES`] — the one the daemon's PTY emulates, as a local agent
/// is told; `env -i` would otherwise leave it none, and an agent with no `TERM` draws without
/// colour), then exactly `env`, then the agent is found by `argv[0]` on the target's `PATH`:
///
/// ```text
/// exec "${SHELL:-/bin/sh}" -lc 'exec env -i HOME="$HOME" PATH="$PATH" TMPDIR="${TMPDIR:-/tmp}" TERM='\''xterm-256color'\'' COLORTERM='\''truecolor'\'' K='\''V'\'' … '\''<agent>'\'' …'
/// ```
pub fn target_login_exec(argv: &[String], env: &[(String, String)]) -> String {
    let mut inner = format!("exec env -i {TARGET_BASE_ENV}");
    for (name, value) in tddy_pty::terminal_variables::TERMINAL_VARIABLES {
        inner.push_str(&format!(" {name}={}", shell_single_quote(value)));
    }
    for (name, value) in env {
        inner.push_str(&format!(" {name}={}", shell_single_quote(value)));
    }
    for argument in argv {
        inner.push_str(&format!(" {}", shell_single_quote(argument)));
    }
    format!(
        "exec \"${{SHELL:-/bin/sh}}\" -lc '{}'",
        inner.replace('\'', "'\\''")
    )
}

/// `argv` run on a target in `worktree`, by [`target_login_exec`]:
/// `cd '<worktree>' && <login exec>`. What a one-shot of the target's own agent (a chat mint) runs.
pub fn target_agent_one_shot(worktree: &Path, argv: &[String], env: &[(String, String)]) -> String {
    format!(
        "cd {} && {}",
        shell_single_quote(&worktree.to_string_lossy()),
        target_login_exec(argv, env)
    )
}

/// The one shell command that starts `session_id`'s agent on a target: enter `worktree`, write the
/// shell's own pid to its [`remote_agent_pid_file`] there (`.tddy/agents/<session id>.pid`, so
/// agents sharing a worktree never overwrite each other's), then `exec` the target's login shell,
/// which `exec`s `argv`
/// under `env -i` with the target's own `HOME`, `PATH` and `TMPDIR` and exactly `env` — so the
/// recorded pid is the agent's, and the agent sees nothing of the remote login's other variables:
///
/// ```text
/// cd '<worktree>' && mkdir -p .tddy/agents && printf %s "$$" > '.tddy/agents/<session id>.pid' && <target_login_exec>
/// ```
///
/// Every path, value and argument is single-quoted for a POSIX shell. `argv[0]` is the agent's
/// bare command, which the target's login `PATH` resolves.
pub fn remote_agent_command(
    worktree: &Path,
    session_id: &str,
    argv: &[String],
    env: &[(String, String)],
) -> String {
    format!(
        "cd {} && mkdir -p .tddy/agents && printf %s \"$$\" > {} && {}",
        shell_single_quote(&worktree.to_string_lossy()),
        shell_single_quote(&remote_agent_pid_file(session_id).to_string_lossy()),
        target_login_exec(argv, env)
    )
}

/// The tmux session `session_id`'s agent runs in on a target: the name a host tmux run uses
/// ([`tddy_cli_sessions::tmux::agent_tmux_session_name`]), `tddy-<session id>`.
pub fn target_tmux_session_name(session_id: &str) -> String {
    tddy_cli_sessions::tmux::agent_tmux_session_name(session_id)
}

/// The one shell command that runs `session_id`'s agent in tmux on a target: enter `worktree`, then
/// the target's login shell runs `tmux new-session -A` for [`target_tmux_session_name`], whose
/// command is the agent's [`target_login_exec`] — so tmux is found on the login `PATH` the probe
/// asked ([`probe_tmux_on_target`]), and a session already running is attached to instead:
///
/// ```text
/// cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux -T clipboard new-session -A -s '\''tddy-<id>'\'' '\''<target_login_exec>'\'''
/// ```
///
/// The client announces the clipboard feature (`-T clipboard`), as a host run's does; clipboard
/// forwarding on the session itself is enabled once the agent is known to have started
/// ([`enable_target_tmux_clipboard`]).
///
/// No pid file is written: the agent is the tmux server's child, and the tmux session's name is
/// what names it.
pub fn remote_agent_tmux_command(
    worktree: &Path,
    session_id: &str,
    argv: &[String],
    env: &[(String, String)],
) -> String {
    let _ = session_id;
    format!(
        "cd {} && {}",
        shell_single_quote(&worktree.to_string_lossy()),
        target_login_run(&format!(
            "exec tmux -T clipboard new-session -A -s {} {}",
            shell_single_quote(&target_tmux_session_name(session_id)),
            shell_single_quote(&target_login_exec(argv, env))
        ))
    )
}

/// `inner` run by the target's login shell: `exec "${SHELL:-/bin/sh}" -lc '<inner>'`, so a command
/// found only on the login `PATH` (tmux, the agent) is the one run.
pub(crate) fn target_login_run(inner: &str) -> String {
    format!(
        "exec \"${{SHELL:-/bin/sh}}\" -lc {}",
        shell_single_quote(inner)
    )
}

/// Kill the tmux session `session_id`'s agent runs in on the target `shell` reaches
/// (`tmux kill-session -t '=tddy-<id>'`, through the target's login shell). A tmux session — or a
/// tmux server — that is already gone is not an error.
pub async fn end_target_tmux_session(
    shell: &dyn RemoteShell,
    session_id: &str,
) -> Result<(), RemoteLaunchError> {
    end_target_tmux_session_named(shell, &target_tmux_session_name(session_id)).await
}

/// Where and how an agent is launched on a target.
pub struct RemoteLaunch<'a> {
    /// The shell that reaches the target ([`shell_for_target`]).
    pub shell: Arc<dyn RemoteShell>,
    /// The session's worktree on the target: the agent's working directory.
    pub worktree: &'a Path,
    /// The session's directory on this host: the working directory of the local PTY that runs the
    /// transport's client. A file the agent's argv names in it is copied into `worktree`'s `.tddy/`
    /// on the target, and the argument rewritten to that copy.
    pub session_dir: &'a Path,
    /// The session pointer already placed in `worktree` on the target, when the launch placed one:
    /// rewritten there with the agent as its owner once the agent has started.
    pub pointer: Option<&'a SessionPointer>,
    /// How long the agent must stay up for the launch to count as started. An agent — or a
    /// transport — that exits within it fails the launch with its last output.
    pub startup_window: Duration,
    /// How long the agent may take to record its pid on the target, from the spawn of its
    /// terminal: the transport's connect time (a Coder workspace's is ~10 s) and the remote
    /// command's start. An agent not started within it fails the launch.
    pub start_timeout: Duration,
    /// The session's clone on the target: the repository root `worktree` was cut from. A Claude
    /// start trusts it in the target's `~/.claude.json` ([`crate::target_trust`]); `None` when the
    /// caller does not know it, and nothing is trusted.
    pub clone_root: Option<&'a Path>,
    /// Run the agent in a tmux session of its own on the target
    /// ([`remote_agent_tmux_command`]), named for the session ([`target_tmux_session_name`]):
    /// a dropped connection leaves it running, and a later launch re-attaches to it.
    pub tmux: bool,
    /// Where that tmux session lives, for the main's `TmuxRun`: the client ending is classified
    /// and watched through it (`#target-reconnect` 2/3). `None` names no location — the run is
    /// then not watched, as a non-tmux run is not.
    pub tmux_location: Option<tddy_cli_sessions::tmux_location::SharedTmuxLocation>,
}

/// The clone `worktree` was cut from, when it sits where a target checkout puts a session's
/// worktree: `<clone root>/.worktrees/<session id>`. `None` for a worktree anywhere else — its
/// clone is not known, and a start there trusts nothing.
pub fn clone_root_of_worktree<'a>(worktree: &'a Path, session_id: &str) -> Option<&'a Path> {
    if worktree.file_name()? != session_id {
        return None;
    }
    let worktrees = worktree.parent()?;
    if worktrees.file_name()? != ".worktrees" {
        return None;
    }
    worktrees.parent()
}

/// An agent started on a target.
pub struct RemoteLaunched {
    /// The local PTY whose child is the transport's client: the session's main terminal. Its pid is
    /// the client's, not the agent's.
    pub handle: Arc<PtyHandle>,
    /// The agent on the target — its pid and the target's name for itself — as recorded in the
    /// pointer there. `None` when the launch placed no pointer.
    pub owner: Option<SessionPointerOwner>,
}

/// Why an agent was not started on its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteLaunchError {
    /// The agent's spec could not be rendered into a launch.
    Plan(SpecLaunchError),
    /// The remote shell failed: the target is unreachable, did not answer, or a command there
    /// failed.
    Shell(RemoteShellError),
    /// The target's `~/.claude.json` could not be made ready for a Claude start.
    Trust(crate::target_trust::TargetTrustError),
    /// The local PTY for the transport's client could not be spawned.
    Spawn(String),
    /// The agent, or the transport's client, exited within the startup window. `status` is the
    /// client's exit status — the agent's own when the agent ran — and `last_output` what its
    /// terminal showed last.
    ExitedDuringStartup {
        status: Option<i32>,
        last_output: String,
    },
    /// The agent never recorded its pid on the target within the bound, though its terminal is
    /// still running: it never started, so there is no startup window to judge it by.
    AgentNeverStarted { after: Duration },
    /// The agent recorded its pid, but no liveness probe of it was answered within the bound
    /// (the startup window plus the start timeout, from the pid's recording) while its terminal
    /// kept running: the target went silent, and the agent can be judged neither up nor down.
    AgentNeverAnswered { after: Duration },
    /// The target's login shell does not find the agent's bare `command` on its `PATH`
    /// ([`probe_agent_on_target`]): the agent is not installed there, and nothing was started.
    AgentNotOnTarget { command: String },
    /// A tmux run was asked for, and the target's login shell finds no `tmux` on its `PATH`
    /// ([`probe_tmux_on_target`]): nothing was started.
    TmuxNotOnTarget,
}

impl fmt::Display for RemoteLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(e) => write!(f, "{e}"),
            Self::Shell(e) => write!(f, "{e}"),
            Self::Trust(e) => write!(f, "the target could not be made ready for the agent: {e}"),
            Self::Spawn(message) => write!(f, "failed to spawn the agent's terminal: {message}"),
            Self::ExitedDuringStartup {
                status: Some(status),
                last_output,
            } => write!(
                f,
                "the agent exited with status {status} while starting: {}",
                last_output.trim()
            ),
            Self::ExitedDuringStartup {
                status: None,
                last_output,
            } => write!(
                f,
                "the agent was killed while starting: {}",
                last_output.trim()
            ),
            Self::AgentNeverStarted { after } => write!(
                f,
                "the agent on the target did not record its pid within {after:?}: it never started, though its terminal is still running"
            ),
            Self::AgentNeverAnswered { after } => write!(
                f,
                "the target did not answer a liveness probe of the agent within {after:?}: its terminal is still running, but the agent can not be confirmed alive"
            ),
            Self::AgentNotOnTarget { command } => write!(
                f,
                "the agent `{command}` is not installed on the target: its login shell finds no `{command}` on its PATH"
            ),
            Self::TmuxNotOnTarget => write!(
                f,
                "run_in_tmux needs tmux on the target: its login shell finds no `tmux` on its PATH"
            ),
        }
    }
}

impl std::error::Error for RemoteLaunchError {}

/// Start `agent` on the target `remote.shell` reaches, in `remote.worktree`, and register the local
/// PTY that is its terminal under `session`.
///
/// The agent's argv is its spec's, rendered as a host launch renders it; its environment is the
/// allow-listed one a host launch computes (`AgentEnvBase::agent_env`: the base set, the spec's
/// `pass_env` values, `run.env` and `session.env`), carried inside [`remote_agent_command`]. The
/// local PTY runs [`RemoteShell::interactive`] over that command in `remote.session_dir`.
///
/// The launch returns once the agent has outlived `remote.startup_window`; by then the agent's pid
/// file is read and removed and `remote.pointer` rewritten on the target with the agent as its
/// owner. An exit inside the window fails the launch and leaves no terminal registered. `on_exit`
/// runs once the client has exited, with its terminal's output.
pub async fn launch_on_target(
    fs: &dyn tddy_fs::Fs,
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    remote: RemoteLaunch<'_>,
    session: HostPtySession<'_>,
    on_exit: Option<ExitHook>,
) -> Result<RemoteLaunched, RemoteLaunchError> {
    clear_the_agent_pid_file(&remote, session.session_id).await?;

    // The slot files are the target's to hold, so they are taken out before the plan can write
    // them on this host, and written through the port at the paths the agent is told.
    let (slot_files, plan) = plan_the_launch(fs, agent, params, slots, &remote).await?;
    // A file the launch rendered into this session's directory on this host (its appended system
    // prompt) is copied into the worktree's `.tddy/` on the target and its argument rewritten to
    // that copy: the target need not have this host's directory, so no host path reaches it.
    let plan =
        write_rendered_session_files(fs, &SessionFilesPlace::of(&remote), slot_files, plan).await?;
    pre_trust_a_claude_clone(agent, &remote).await?;

    let client = client_argv_and_env(agent, &remote, &session, plan);
    // A tmux run's main is a client of its tmux session at its location: a dropped transport
    // is classified and watched there, not read as the agent ending (`#target-reconnect` 2/3).
    let tmux_run = if remote.tmux {
        tddy_cli_sessions::cli_session_manager::TmuxRun::with_location(
            Some(&target_tmux_session_name(session.session_id)),
            session.os_user,
            remote.tmux_location.clone(),
        )
    } else {
        tddy_cli_sessions::cli_session_manager::TmuxRun::with_location(None, session.os_user, None)
    };

    let handle = session
        .manager
        .spawn_rendered_client_main(
            session.session_id,
            tddy_cli_sessions::cli_session_manager::ClientMain {
                session_type: session.session_type.to_string(),
                worktree_path: remote.session_dir.to_path_buf(),
                model: session.model.to_string(),
                argv: client.argv,
                env: client.env,
                os_user: session.os_user.map(str::to_string),
                on_exit,
                exit_code_in_output: client.exit_code_in_output,
                tmux_run,
            },
        )
        .await
        .map_err(|error| RemoteLaunchError::Spawn(error.to_string()))?;

    // The startup window runs from when the agent is known to have started, not from the PTY
    // spawn: under load the remote shell may not have run the remote command yet when a
    // spawn-counted window elapses, and an agent that exits at once would be judged started.
    // The remote command writes the session's pid file immediately before it execs the agent.
    let in_tmux = remote.tmux;
    let shell = Arc::clone(&remote.shell);
    let session_id = session.session_id;
    let launched = judge_the_start(remote, session, handle).await?;
    if in_tmux {
        // The agent is known to have started, so its tmux session lives; a session that refuses
        // still runs its agent, and the failure is logged, not fatal.
        let name = target_tmux_session_name(session_id);
        enable_target_tmux_clipboard_logged(shell.as_ref(), &name).await;
    }
    Ok(launched)
}

/// The directory `session_id`'s agent pid file is written into, created through the port before
/// any terminal is registered, with no pid left in it from an earlier start of the session: a
/// target that cannot be reached fails here, with the shell's own reason, and leaves no session
/// behind.
async fn clear_the_agent_pid_file(
    remote: &RemoteLaunch<'_>,
    session_id: &str,
) -> Result<(), RemoteLaunchError> {
    let pid_file = remote.worktree.join(remote_agent_pid_file(session_id));
    remote
        .shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "mkdir -p {} && rm -f {}",
            shell_single_quote(&remote.worktree.join(".tddy/agents").to_string_lossy()),
            shell_single_quote(&pid_file.to_string_lossy())
        )))
        .await
        .map_err(RemoteLaunchError::Shell)?
        .success_or_exited()
        .map_err(RemoteLaunchError::Shell)?;
    Ok(())
}

/// A Claude start on a target meets neither the folder-trust nor the project-MCP dialog: the
/// target's `~/.claude.json` records both answers first. Only for a spec whose hooks adapter is
/// Claude's, and only when the caller named the clone root.
async fn pre_trust_a_claude_clone(
    agent: &HostAgent,
    remote: &RemoteLaunch<'_>,
) -> Result<(), RemoteLaunchError> {
    let Some(clone_root) = remote.clone_root else {
        return Ok(());
    };
    if agent.spec.agent.hooks.adapter
        != tddy_coding_agent_specs::slots::hooks::CLAUDE_SETTINGS_LOCAL
    {
        return Ok(());
    }
    crate::target_trust::pre_trust_clone_root(remote.shell.as_ref(), clone_root, remote.worktree)
        .await
        .map(drop)
        .map_err(RemoteLaunchError::Trust)
}

/// The local process that is a target agent's terminal: the transport's client, its environment,
/// and how the agent's status is read from it once it has exited.
struct TransportClient {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    exit_code_in_output: Option<tddy_pty::ExitCodeInOutput>,
}

fn client_argv_and_env(
    agent: &HostAgent,
    remote: &RemoteLaunch<'_>,
    session: &HostPtySession<'_>,
    plan: crate::spec_launch::HostPtyPlan,
) -> TransportClient {
    let command = remote_command(agent, remote, session, plan);
    client_command(remote, command)
}

fn remote_command(
    agent: &HostAgent,
    remote: &RemoteLaunch<'_>,
    session: &HostPtySession<'_>,
    plan: crate::spec_launch::HostPtyPlan,
) -> String {
    let env: Vec<(String, String)> = plan
        .env
        .into_iter()
        .chain(session.env.iter().cloned())
        .collect();
    // Only what the daemon passes on purpose travels: the spec's `pass_env` values and the launch's
    // own. The base set (`HOME`, `PATH`, `TMPDIR`) is the target's, expanded there.
    let mut passed = agent.spec.passed_env(std::env::vars_os());
    passed.extend(env);
    let env = passed;

    if remote.tmux {
        remote_agent_tmux_command(remote.worktree, session.session_id, &plan.argv, &env)
    } else {
        remote_agent_command(remote.worktree, session.session_id, &plan.argv, &env)
    }
}

/// Where the launch reads the agent's pid on the target: its tmux pane for a tmux run, the pid
/// file its command wrote otherwise.
fn agent_pid_source(remote: &RemoteLaunch<'_>, session_id: &str) -> RemoteAgentPid {
    if remote.tmux {
        RemoteAgentPid::TmuxPane(target_tmux_session_name(session_id))
    } else {
        RemoteAgentPid::File(remote.worktree.join(remote_agent_pid_file(session_id)))
    }
}

fn client_command(remote: &RemoteLaunch<'_>, command: String) -> TransportClient {
    transport_client(remote.shell.as_ref(), &command)
}

/// The transport client of `shell` that runs `command` on its target, as a local process.
fn transport_client(shell: &dyn RemoteShell, command: &str) -> TransportClient {
    let interactive = shell.interactive(command);
    let argv: Vec<String> = interactive
        .argv
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    // The local PTY's child is the transport's client: it carries this daemon's ssh environment
    // (its agent socket) and the `PATH` a bare client (`coder`, `ssh`) is found on — the PTY's
    // environment is otherwise cleared — never the agent's, which already travelled inside the
    // command.
    let mut local_env: Vec<(String, String)> = interactive
        .env
        .iter()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    for name in ["SSH_AUTH_SOCK", "PATH"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() && !local_env.iter().any(|(set, _)| set == name) {
                local_env.push((name.to_string(), value));
            }
        }
    }
    // A client that exits with one code for any failed command (Coder's) names the agent's status
    // in its output; the terminal's status is read from there, so it is the agent's own.
    let exit_code_in_output = match interactive.exit {
        InteractiveExit::ClientStatus => None,
        InteractiveExit::ReportedLast {
            client_code,
            marker,
        } => Some(tddy_pty::ExitCodeInOutput {
            client_code,
            marker: marker.to_string(),
        }),
    };
    TransportClient {
        argv,
        env: local_env,
        exit_code_in_output,
    }
}

async fn judge_the_start(
    remote: RemoteLaunch<'_>,
    session: HostPtySession<'_>,
    handle: Arc<PtyHandle>,
) -> Result<RemoteLaunched, RemoteLaunchError> {
    let start_bound = remote.start_timeout;
    let agent_pid = match wait_for_agent_start(
        remote.shell.as_ref(),
        &agent_pid_source(&remote, session.session_id),
        &handle,
        start_bound,
    )
    .await
    {
        AgentStart::Started(pid) => Some(pid),
        AgentStart::Exited => None,
        AgentStart::NeverStarted => {
            session.manager.stop_session(session.session_id).await;
            return Err(RemoteLaunchError::AgentNeverStarted { after: start_bound });
        }
    };

    let judgement = exits_during_startup(
        &handle,
        remote.startup_window,
        remote.shell.as_ref(),
        agent_pid,
        start_bound,
    )
    .await;
    if judgement == StartupJudgement::NeverAnswered {
        session.manager.stop_session(session.session_id).await;
        return Err(RemoteLaunchError::AgentNeverAnswered {
            after: remote.startup_window + start_bound,
        });
    }
    if judgement == StartupJudgement::Exited {
        // The output settles first: an agent judged exited by its pid may not have its exit
        // status recorded yet, and settling the capture gives the terminal time to record it.
        let last_output = settled_last_output(&handle).await;
        let status = match handle.task_status() {
            tddy_task::TaskStatus::Completed { exit_code } => exit_code,
            _ => None,
        };
        // The process has ended; removing its registry entry now leaves no terminal behind.
        session.manager.stop_session(session.session_id).await;
        return Err(RemoteLaunchError::ExitedDuringStartup {
            status,
            last_output,
        });
    }

    let owner = recorded_owner(remote, session.session_id).await?;

    Ok(RemoteLaunched { handle, owner })
}

async fn recorded_owner(
    remote: RemoteLaunch<'_>,
    session_id: &str,
) -> Result<Option<SessionPointerOwner>, RemoteLaunchError> {
    let owner = match remote.pointer {
        Some(pointer) => {
            let placed = PlacedSessionPointer {
                path: session_pointer_path(remote.worktree),
                host: PointerHost::Remote {
                    shell: Arc::clone(&remote.shell),
                },
            };
            Some(
                record_remote_owner_from(&placed, pointer, &agent_pid_source(&remote, session_id))
                    .await
                    .map_err(|status| {
                        RemoteLaunchError::Shell(RemoteShellError::Spawn(format!(
                            "the session pointer's owner could not be recorded: {}",
                            status.message()
                        )))
                    })?,
            )
        }
        None => None,
    };
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tddy_remote_shell::ShellAddress;

    /// A variable the "remote login" holds and the agent must not see.
    const LOGIN_ONLY: &str = "REMOTE_LOGIN_ONLY";

    /// What the target's login has for every command it runs, as `printed_by` sets it.
    const TARGET_HOME: &str = "/home/target";
    const TARGET_PATH: &str = "/usr/bin:/bin";
    const TARGET_TMPDIR: &str = "/target/tmp";

    /// A stand-in for the target's login shell: it is run as `$SHELL -lc <command>`, as the remote
    /// command runs it, and runs the command under the login environment its caller set, so a
    /// test never depends on what this host's own `zsh -l` or `sh -l` reads.
    fn a_login_shell() -> &'static Path {
        static LOGIN_SHELL: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        LOGIN_SHELL.get_or_init(|| {
            use std::os::unix::fs::PermissionsExt as _;
            let dir = std::env::temp_dir().join(format!("tddy-login-shell-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("a directory for the stand-in login shell");
            let path = dir.join("login-shell");
            // The warm-up guard and the warm-up run: a freshly written executable pays a
            // first-execution scan on macOS, which a launch's timing must not absorb.
            std::fs::write(
                &path,
                "#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n\
                 [ \"$1\" = -lc ] || exit 64\nexec /bin/sh -c \"$2\"\n",
            )
            .expect("the stand-in login shell is written");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("the stand-in login shell is made executable");
            let warmed = std::process::Command::new(&path)
                .env("TDDY_FIXTURE_WARMUP", "1")
                .output()
                .expect("the stand-in login shell warms up");
            assert!(
                warmed.status.success(),
                "the warm-up of the login shell failed"
            );
            path
        })
    }

    /// What `command` printed when a POSIX shell ran it on this host, as a remote shell would: as
    /// a child of a login that holds [`LOGIN_ONLY`] and the target's `HOME`, `PATH` and `TMPDIR`,
    /// whose shell is [`a_login_shell`].
    fn printed_by(command: &str) -> String {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .env_clear()
            .env("SHELL", a_login_shell())
            .env("HOME", TARGET_HOME)
            .env("PATH", TARGET_PATH)
            .env("TMPDIR", TARGET_TMPDIR)
            .env(LOGIN_ONLY, "leaked")
            .output()
            .expect("a shell runs on this host");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A directory standing for a worktree on the target, by its canonical path.
    fn a_worktree_named(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("the temp dir resolves");
        let worktree = root.join(name);
        std::fs::create_dir_all(&worktree).expect("the worktree is created");
        (dir, worktree)
    }

    /// What `command` printed when run as [`printed_by`] runs it, on a target whose `PATH` also
    /// holds a stand-in `tmux`: it prints its arguments but the last on one line, then runs the
    /// last — the session's command — with `sh -c`, as a new tmux session would.
    fn printed_through_a_stand_in_tmux(command: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("a directory for the stand-in tmux");
        let tmux = dir.path().join("tmux");
        std::fs::write(
            &tmux,
            "#!/bin/sh\nn=$#\ni=1\nline=\nfor a; do\n  if [ $i -lt $n ]; then line=\"$line${line:+ }$a\"; else last=$a; fi\n  i=$((i+1))\ndone\nprintf '%s\\n' \"$line\"\nexec /bin/sh -c \"$last\"\n",
        )
        .expect("the stand-in tmux is written");
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
            .expect("the stand-in tmux is made executable");
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .env_clear()
            .env("SHELL", a_login_shell())
            .env("HOME", TARGET_HOME)
            .env("PATH", format!("{}:{TARGET_PATH}", dir.path().display()))
            .env("TMPDIR", TARGET_TMPDIR)
            .output()
            .expect("a shell runs on this host");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn the_tmux_command_runs_the_agent_in_a_tmux_session_named_for_the_session_in_the_worktree() {
        // Given an agent that prints its working directory and an argument
        let (_dir, worktree) = a_worktree_named("s1");
        let argv = strings(&["sh", "-c", "/bin/pwd -P; echo \"$0\"", "it's up"]);

        // When the tmux command runs on the target
        let printed = printed_through_a_stand_in_tmux(&remote_agent_tmux_command(
            &worktree,
            "s1",
            &argv,
            &[],
        ));

        // Then tmux was asked to attach to or create the session's tmux session, whose command ran
        // the agent in the worktree with its argument intact — and no pid file was written
        assert_eq!(
            (printed, worktree.join(remote_agent_pid_file("s1")).exists()),
            (
                format!(
                    "-T clipboard new-session -A -s tddy-s1\n{}\nit's up\n",
                    worktree.display()
                ),
                false
            )
        );
    }

    #[test]
    fn the_remote_command_enters_the_worktree_records_the_pid_and_execs_the_agent_under_its_environment(
    ) {
        // Given the agent's bare command, as a target's own login shell finds it
        let worktree = Path::new("/home/dev/repo/.worktrees/s1");
        let argv = strings(&["claude", "--model", "opus"]);
        let env = [("ANTHROPIC_MODEL".to_string(), "opus".to_string())];

        // When
        let command = remote_agent_command(worktree, "s1", &argv, &env);

        // Then the target's login shell runs `env -i` with the base set expanded there (`HOME`,
        // `PATH`, `TMPDIR`), the terminal the daemon's PTY emulates, then only the explicit
        // environment, then the bare command
        assert_eq!(
            command,
            r#"cd '/home/dev/repo/.worktrees/s1' && mkdir -p .tddy/agents && printf %s "$$" > '.tddy/agents/s1.pid' && exec "${SHELL:-/bin/sh}" -lc 'exec env -i HOME="$HOME" PATH="$PATH" TMPDIR="${TMPDIR:-/tmp}" TERM='\''xterm-256color'\'' COLORTERM='\''truecolor'\'' ANTHROPIC_MODEL='\''opus'\'' '\''claude'\'' '\''--model'\'' '\''opus'\'''"#
        );
    }

    #[test]
    fn the_remote_command_quotes_a_worktree_path_with_a_space() {
        // Given a worktree whose path holds a space
        let (_dir, worktree) = a_worktree_named("my repo");
        let argv = strings(&["/bin/pwd", "-P"]);

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, "s1", &argv, &[]));

        // Then the agent ran in that worktree
        assert_eq!(printed.trim_end(), worktree.to_string_lossy());
    }

    /// Findings: docs/dev/changesets/2026-11-22-coder-target-back-channel.md, problem 6
    /// — on 2026-11-22 the Coder probe's terminal had no colour run at all (`ReadSessionTerminal`
    /// `styles: []` on every row) while the local probe's had them: `env -i` leaves the agent no
    /// `TERM`, so Claude Code renders monochrome. The local route's PTY names its terminal
    /// (`tddy_pty::runtime::set_terminal_variables`); the target's agent must be told the same.
    #[test]
    fn the_agent_on_a_target_is_told_the_terminal_it_draws_on() {
        // Given an agent that prints the terminal it was told it runs in
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/bin/sh", "-c", r#"printf '%s|%s' "$TERM" "$COLORTERM""#]);

        // When its remote command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, "s1", &argv, &[]));

        // Then it is the terminal the daemon's PTY emulates, colours included
        assert_eq!(printed, "xterm-256color|truecolor");
    }

    #[test]
    fn an_environment_value_with_a_quote_survives_the_remote_shell() {
        // Given a value holding both quotes, a variable reference and a command separator
        let (_dir, worktree) = a_worktree_named("worktree");
        let value = r#"it's "5 o'clock" in $HOME; echo `id`"#;
        let argv = strings(&["/bin/sh", "-c", r#"printf %s "$GREETING""#]);
        let env = [("GREETING".to_string(), value.to_string())];

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, "s1", &argv, &env));

        // Then the agent reads the value byte for byte
        assert_eq!(printed, value);
    }

    #[test]
    fn an_argument_with_a_quote_and_a_space_reaches_the_agent_as_one_argument() {
        // Given a prompt holding a quote and spaces
        let (_dir, worktree) = a_worktree_named("worktree");
        let prompt = "fix the user's login; then stop";
        let argv = strings(&[
            "/bin/sh",
            "-c",
            r#"printf '%s|' "$@""#,
            "agent",
            "--prompt",
            prompt,
        ]);

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, "s1", &argv, &[]));

        // Then
        assert_eq!(printed, format!("--prompt|{prompt}|"));
    }

    #[test]
    fn the_agent_sees_the_targets_base_set_and_only_the_environment_the_command_carries() {
        // Given a login that holds a variable of its own, and a command carrying one variable
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/usr/bin/env"]);
        let env = [(
            "ROUTE_BASE_URL".to_string(),
            "https://route.example".to_string(),
        )];

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, "s1", &argv, &env));
        let mut seen: Vec<&str> = printed.lines().collect();
        seen.sort_unstable();

        // Then the agent has the target's `HOME`, `PATH` and `TMPDIR`, as the target's login
        // shell expanded them, the terminal the daemon's PTY emulates, and the one variable the
        // command carries, and nothing of the login
        assert_eq!(
            seen,
            vec![
                "COLORTERM=truecolor",
                "HOME=/home/target",
                "PATH=/usr/bin:/bin",
                "ROUTE_BASE_URL=https://route.example",
                "TERM=xterm-256color",
                "TMPDIR=/target/tmp",
            ]
        );
    }

    #[test]
    fn the_pid_file_is_written_before_the_agent_replaces_the_shell() {
        // Given an agent that prints its own pid
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/bin/sh", "-c", r#"printf %s "$$""#]);

        // When the command is run by a shell
        let agent_pid = printed_by(&remote_agent_command(&worktree, "s1", &argv, &[]));
        let recorded =
            std::fs::read_to_string(worktree.join(".tddy/agents/s1.pid")).unwrap_or_default();

        // Then the file holds the agent's pid: the shell that wrote it became the agent
        assert_eq!(
            (recorded.as_str(), recorded.parse::<u32>().is_ok()),
            (agent_pid.as_str(), true)
        );
    }

    #[test]
    fn agents_sharing_a_worktree_each_record_their_own_pid() {
        // Given two sessions' agents started in one worktree, each printing its own pid
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/bin/sh", "-c", r#"printf %s "$$""#]);

        // When both commands are run by a shell
        let first_pid = printed_by(&remote_agent_command(&worktree, "first", &argv, &[]));
        let second_pid = printed_by(&remote_agent_command(&worktree, "second", &argv, &[]));
        let recorded = |session_id: &str| {
            std::fs::read_to_string(worktree.join(remote_agent_pid_file(session_id)))
                .unwrap_or_default()
        };

        // Then each session's file holds its own agent's pid: the second did not overwrite the first
        assert_ne!(first_pid, second_pid);
        assert_eq!(
            (recorded("first"), recorded("second")),
            (first_pid, second_pid)
        );
    }

    #[test]
    fn an_ssh_target_is_reached_through_an_ssh_shell_on_its_alias() {
        // Given
        let target = SessionTarget::ssh("buildbox");

        // When
        let address = shell_for_target(&target).map(|shell| shell.address());

        // Then
        assert_eq!(address, Ok(ShellAddress::new("ssh", "buildbox")));
    }

    #[test]
    fn an_ssh_target_naming_no_host_is_refused() {
        // Given
        let target = SessionTarget::ssh("  ");

        // When
        let address = shell_for_target(&target).map(|shell| shell.address());

        // Then
        assert_eq!(address, Err(TargetError::NoSshHost));
    }

    #[test]
    fn an_early_exit_reads_as_the_agents_status_and_its_last_output() {
        // Given
        let error = RemoteLaunchError::ExitedDuringStartup {
            status: Some(3),
            last_output: "acme: not logged in\r\n".to_string(),
        };

        // When
        let shown = error.to_string();

        // Then
        assert_eq!(
            shown,
            "the agent exited with status 3 while starting: acme: not logged in"
        );
    }
}
