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

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_actions::tool::TemplateParams;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner};
use tddy_core::SessionTarget;
use tddy_remote_shell::{RemoteShell, RemoteShellError};

use crate::spec_launch::{HostAgent, HostPtySession, SpecLaunchError};

/// Why a session's target cannot be reached through a remote shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// An ssh target that names no `Host` alias.
    NoSshHost,
    /// A target this daemon has no transport for, with the reason.
    Unsupported(String),
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSshHost => write!(f, "the ssh target names no host alias"),
            Self::Unsupported(reason) => write!(f, "the target cannot be reached: {reason}"),
        }
    }
}

impl std::error::Error for TargetError {}

/// The remote shell that reaches `target`: for an ssh target, the OpenSSH transport on its alias,
/// through the `ssh` on the daemon's `PATH`.
pub fn shell_for_target(target: &SessionTarget) -> Result<Arc<dyn RemoteShell>, TargetError> {
    // TODO(ssh-agent-launch): implement
    let _ = target;
    Err(TargetError::Unsupported(
        "no remote shell is built for a session target yet".to_string(),
    ))
}

/// The one shell command that starts an agent on a target: enter `worktree`, write the shell's own
/// pid to `.tddy/agent.pid` there, then `exec` `argv` under `env -i` with exactly `env` — so the
/// recorded pid is the agent's, and the agent sees nothing of the remote login's environment:
///
/// ```text
/// cd '<worktree>' && mkdir -p .tddy && printf %s "$$" > .tddy/agent.pid && exec env -i K='V' … '<agent>' '<arg>' …
/// ```
///
/// Every path, value and argument is single-quoted for a POSIX shell.
pub fn remote_agent_command(worktree: &Path, argv: &[String], env: &[(String, String)]) -> String {
    // TODO(ssh-agent-launch): implement
    let _ = (worktree, argv, env);
    String::new()
}

/// Where and how an agent is launched on a target.
pub struct RemoteLaunch<'a> {
    /// The shell that reaches the target ([`shell_for_target`]).
    pub shell: Arc<dyn RemoteShell>,
    /// The session's worktree on the target: the agent's working directory.
    pub worktree: &'a Path,
    /// The session's directory on this host: the working directory of the local PTY that runs the
    /// transport's client.
    pub session_dir: &'a Path,
    /// The session pointer already placed in `worktree` on the target, when the launch placed one:
    /// rewritten there with the agent as its owner once the agent has started.
    pub pointer: Option<&'a SessionPointer>,
    /// How long the agent must stay up for the launch to count as started. An agent — or a
    /// transport — that exits within it fails the launch with its last output.
    pub startup_window: Duration,
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
    /// The local PTY for the transport's client could not be spawned.
    Spawn(String),
    /// The agent, or the transport's client, exited within the startup window. `status` is the
    /// client's exit status — the agent's own when the agent ran — and `last_output` what its
    /// terminal showed last.
    ExitedDuringStartup {
        status: Option<i32>,
        last_output: String,
    },
}

impl fmt::Display for RemoteLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(e) => write!(f, "{e}"),
            Self::Shell(e) => write!(f, "{e}"),
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
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    remote: RemoteLaunch<'_>,
    session: HostPtySession<'_>,
    on_exit: Option<ExitHook>,
) -> Result<RemoteLaunched, RemoteLaunchError> {
    // TODO(ssh-agent-launch): implement
    let _ = (agent, params, slots, remote, session, on_exit);
    Err(RemoteLaunchError::Spawn(
        "launching an agent on a target is not implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tddy_remote_shell::ShellAddress;

    /// A variable the "remote login" holds and the agent must not see.
    const LOGIN_ONLY: &str = "REMOTE_LOGIN_ONLY";

    /// What `command` printed when a POSIX shell ran it on this host, as a remote shell would.
    fn printed_by(command: &str) -> String {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
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

    #[test]
    fn the_remote_command_enters_the_worktree_records_the_pid_and_execs_the_agent_under_its_environment(
    ) {
        // Given
        let worktree = Path::new("/home/dev/repo/.worktrees/s1");
        let argv = strings(&["/usr/local/bin/claude", "--model", "opus"]);
        let env = [("ANTHROPIC_MODEL".to_string(), "opus".to_string())];

        // When
        let command = remote_agent_command(worktree, &argv, &env);

        // Then
        assert_eq!(
            command,
            "cd '/home/dev/repo/.worktrees/s1' && mkdir -p .tddy && \
             printf %s \"$$\" > .tddy/agent.pid && \
             exec env -i ANTHROPIC_MODEL='opus' '/usr/local/bin/claude' '--model' 'opus'"
        );
    }

    #[test]
    fn the_remote_command_quotes_a_worktree_path_with_a_space() {
        // Given a worktree whose path holds a space
        let (_dir, worktree) = a_worktree_named("my repo");
        let argv = strings(&["/bin/pwd", "-P"]);

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, &argv, &[]));

        // Then the agent ran in that worktree
        assert_eq!(printed.trim_end(), worktree.to_string_lossy());
    }

    #[test]
    fn an_environment_value_with_a_quote_survives_the_remote_shell() {
        // Given a value holding both quotes, a variable reference and a command separator
        let (_dir, worktree) = a_worktree_named("worktree");
        let value = r#"it's "5 o'clock" in $HOME; echo `id`"#;
        let argv = strings(&["/bin/sh", "-c", r#"printf %s "$GREETING""#]);
        let env = [("GREETING".to_string(), value.to_string())];

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, &argv, &env));

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
        let printed = printed_by(&remote_agent_command(&worktree, &argv, &[]));

        // Then
        assert_eq!(printed, format!("--prompt|{prompt}|"));
    }

    #[test]
    fn the_agent_sees_only_the_environment_the_command_carries() {
        // Given a login that holds a variable of its own, and a command carrying one variable
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/usr/bin/env"]);
        let env = [(
            "ROUTE_BASE_URL".to_string(),
            "https://route.example".to_string(),
        )];

        // When the command is run by a shell
        let printed = printed_by(&remote_agent_command(&worktree, &argv, &env));

        // Then
        assert_eq!(printed, "ROUTE_BASE_URL=https://route.example\n");
    }

    #[test]
    fn the_pid_file_is_written_before_the_agent_replaces_the_shell() {
        // Given an agent that prints its own pid
        let (_dir, worktree) = a_worktree_named("worktree");
        let argv = strings(&["/bin/sh", "-c", r#"printf %s "$$""#]);

        // When the command is run by a shell
        let agent_pid = printed_by(&remote_agent_command(&worktree, &argv, &[]));
        let recorded =
            std::fs::read_to_string(worktree.join(".tddy/agent.pid")).unwrap_or_default();

        // Then the file holds the agent's pid: the shell that wrote it became the agent
        assert_eq!(
            (recorded.as_str(), recorded.parse::<u32>().is_ok()),
            (agent_pid.as_str(), true)
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
