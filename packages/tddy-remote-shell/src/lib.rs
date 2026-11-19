//! The remote-shell port: how the daemon runs something on a machine it does not serve.
//!
//! One trait, [`RemoteShell`], with three shapes of "run this there":
//!
//! - [`RemoteShell::exec`]: one command, optional stdin, a wall-clock deadline, its output and
//!   exit status back.
//! - [`RemoteShell::exec_streamed`]: a long-running command with its three pipes open
//!   ([`StreamedExec`]) — what a git pack verb needs.
//! - [`RemoteShell::interactive`]: the argv a **local** PTY runs to be the command's terminal
//!   ([`InteractiveCommand`]). The port opens no PTY of its own.
//!
//! [`SshShell`] is the OpenSSH transport, and the only place this workspace spawns `ssh` for a
//! remote command. [`CoderShell`] is the Coder transport: a workspace of a Coder deployment,
//! reached through the `coder` CLI. A failure says which of four things happened
//! ([`RemoteShellError`]).
//!
//! Behind the `test-support` feature: `InMemoryShell`, a scriptable double, and
//! `contract::assert_behaves_like_a_remote_shell`, the behaviour every transport is held to.

use std::ffi::OsString;
use std::fmt;
use std::time::Duration;

use async_trait::async_trait;

mod child;
mod coder;
mod error;
mod local;
mod recorded;
mod redact;
mod ssh;
mod streamed;

#[cfg(feature = "test-support")]
pub mod contract;
#[cfg(feature = "test-support")]
mod in_memory;

pub use coder::{CoderCredentials, CoderShell, CoderWorkspaceRef};
pub use error::RemoteShellError;
pub use local::LocalShell;
pub use recorded::{RecordedShell, RemoteCallOutcome, RemoteCallReport, RemoteCallSink};
pub use redact::{command_head, scrub_credentials};
pub use ssh::SshShell;
pub use streamed::{ShellOutput, ShellStdin, StreamedExec, StreamedExit};

#[cfg(feature = "test-support")]
pub use in_memory::{InMemoryShell, ScriptedStream, ShellCall};

/// The deadline an [`ExecRequest`] carries unless it is given one.
pub const DEFAULT_EXEC_DEADLINE: Duration = Duration::from_secs(60);

/// Which target a shell reaches, for errors and logs: the transport's tag and the target's name
/// as that transport knows it (an ssh `Host` alias).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShellAddress {
    /// The transport, e.g. `ssh`. A string, not an enum: a later transport adds its own tag
    /// without changing this type.
    pub transport: String,
    /// The target's name on that transport.
    pub name: String,
}

impl ShellAddress {
    pub fn new(transport: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            transport: transport.into(),
            name: name.into(),
        }
    }
}

impl fmt::Display for ShellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} target {}", self.transport, self.name)
    }
}

/// One command to run to completion on the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecRequest {
    /// A POSIX shell command line, run by the target's shell.
    pub command: String,
    /// What the command reads on its standard input; `None` gives it an empty one.
    pub stdin: Option<Vec<u8>>,
    /// The wall-clock bound on the whole call, connecting included. Past it the command is stopped
    /// and the call fails [`RemoteShellError::TimedOut`].
    pub deadline: Duration,
}

impl ExecRequest {
    /// `command` with no stdin, bounded by [`DEFAULT_EXEC_DEADLINE`].
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            stdin: None,
            deadline: DEFAULT_EXEC_DEADLINE,
        }
    }

    /// This request with `stdin` on the command's standard input.
    pub fn with_stdin(mut self, stdin: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// This request bounded by `deadline`.
    pub fn within(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }
}

/// What a command that ran to completion left behind. A non-zero `status` is still an output:
/// [`RemoteShell::exec`] returns it as `Ok`, and a caller that needs success asks
/// [`ExecOutput::success_or_exited`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ExecOutput {
    /// A command that exited zero having printed `stdout`.
    pub fn succeeded(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    /// A command that exited `status` having written `stderr`.
    pub fn exited(status: i32, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            stdout: Vec::new(),
            stderr: stderr.into(),
        }
    }

    /// This output when the command exited zero; otherwise [`RemoteShellError::Exited`] carrying
    /// the status and what the command wrote to stderr.
    pub fn success_or_exited(self) -> Result<Self, RemoteShellError> {
        if self.status == 0 {
            Ok(self)
        } else {
            Err(RemoteShellError::Exited {
                status: self.status,
                stderr: String::from_utf8_lossy(&self.stderr).into_owned(),
            })
        }
    }
}

/// The local command line that is a remote command's terminal: a caller runs `argv`, with `env`
/// added to its environment, inside a local PTY, and reads the remote command's status from it as
/// `exit` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractiveCommand {
    pub argv: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub exit: InteractiveExit,
}

/// How the remote command's status is read once the local client of an [`InteractiveCommand`] has
/// exited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InteractiveExit {
    /// The client exits with the command's status (OpenSSH).
    #[default]
    ClientStatus,
    /// The client exits `client_code` for any command that failed, and prints that command's status
    /// last, as the decimal right after `marker` (Coder: [`CoderShell::FAILED_COMMAND_STATUS`]).
    ReportedLast {
        client_code: i32,
        marker: &'static str,
    },
}

/// The permissions a file written through the port is created with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    /// Whatever the target's own umask gives a new file.
    TargetDefault,
    /// Readable and writable by its owner alone (`0600`).
    OwnerOnly,
}

/// A shell on a machine the daemon does not serve.
#[async_trait]
pub trait RemoteShell: Send + Sync {
    /// Which target this shell reaches.
    fn address(&self) -> ShellAddress;

    /// Run `request.command` to completion. A command that ran is `Ok` whatever its exit status;
    /// `Err` says the target was not reached, the deadline passed, or nothing could be started.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError>;

    /// Start `command` with its three pipes open. Dropping the [`StreamedExec`] stops it.
    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError>;

    /// The local command line a PTY runs to be `command`'s terminal on the target.
    fn interactive(&self, command: &str) -> InteractiveCommand;

    /// The bytes of the file at `path` on the target. A file that cannot be read is
    /// [`RemoteShellError::Exited`] with what the target said.
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteShellError> {
        let command = format!("cat {}", shell_single_quote(path));
        Ok(self
            .exec(ExecRequest::new(command))
            .await?
            .success_or_exited()?
            .stdout)
    }

    /// Write `bytes` to `path` on the target, creating its directory, with the permissions `mode`
    /// names. The bytes travel on the command's stdin, never on its command line.
    async fn write_file(
        &self,
        path: &str,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), RemoteShellError> {
        let dir = match path.rfind('/') {
            Some(0) => "/",
            Some(index) => &path[..index],
            None => ".",
        };
        let quoted_dir = shell_single_quote(dir);
        let quoted_path = shell_single_quote(path);
        let command = match mode {
            FileMode::OwnerOnly => {
                format!("umask 077; mkdir -p {quoted_dir} && cat > {quoted_path}")
            }
            FileMode::TargetDefault => {
                format!("mkdir -p {quoted_dir} && cat > {quoted_path}")
            }
        };
        self.exec(ExecRequest::new(command).with_stdin(bytes.to_vec()))
            .await?
            .success_or_exited()?;
        Ok(())
    }
}

/// Escape a string for a single-quoted POSIX shell argument.
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn an_address_reads_as_its_transport_and_target() {
        // Given
        let address = ShellAddress::new("ssh", "buildbox");

        // When
        let shown = address.to_string();

        // Then
        assert_eq!(shown, "ssh target buildbox");
    }

    #[test]
    fn a_request_runs_without_stdin_within_the_default_deadline_unless_told_otherwise() {
        // Given / When
        let plain = ExecRequest::new("true");
        let fed = ExecRequest::new("cat")
            .with_stdin("in")
            .within(Duration::from_secs(1));

        // Then
        assert_eq!((plain.stdin, plain.deadline), (None, DEFAULT_EXEC_DEADLINE));
        assert_eq!(
            (fed.stdin, fed.deadline),
            (Some(b"in".to_vec()), Duration::from_secs(1))
        );
    }

    #[test]
    fn an_output_that_exited_zero_is_a_success() {
        // Given
        let output = ExecOutput::succeeded("done\n");

        // When
        let checked = output.clone().success_or_exited();

        // Then
        assert_eq!(checked, Ok(output));
    }

    #[test]
    fn an_output_that_exited_non_zero_is_an_exited_error_carrying_its_stderr() {
        // Given
        let output = ExecOutput::exited(3, "fatal: not a git repository\n");

        // When
        let checked = output.success_or_exited();

        // Then
        assert_eq!(
            checked,
            Err(RemoteShellError::Exited {
                status: 3,
                stderr: "fatal: not a git repository\n".to_string(),
            })
        );
    }
}
