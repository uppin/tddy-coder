//! A Coder workspace as a remote-shell transport: the `coder` CLI on this host, given the
//! deployment's URL and a session token in its own environment and nowhere else.
//!
//! `coder ssh` is run with `--disable-autostart` always: this transport never starts a workspace.
//! One that is not running is [`RemoteShellError::Unreachable`], with what the CLI said.

mod one_shot;

use std::fmt;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_credentials::SecretString;

use crate::coder::one_shot::one_shot_command;
use crate::coder::one_shot::read_noting_the_marker;
use crate::coder::one_shot::stdin_lines;
use crate::coder::one_shot::strip_start_marker;
use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, InteractiveExit, RemoteShell, RemoteShellError,
    ShellAddress, StreamedExec,
};

/// What the `coder` CLI needs to reach a deployment: where it is, and the session token to
/// present there.
///
/// Held in memory for the life of the shell that owns it. `Debug` shows the URL and never the
/// token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderCredentials {
    /// The deployment's root, e.g. `https://coder.example.com`, as the CLI takes it in `CODER_URL`.
    pub url: String,
    /// The session token, as the CLI takes it in `CODER_SESSION_TOKEN`.
    pub token: SecretString,
}

impl CoderCredentials {
    pub fn new(url: impl Into<String>, token: SecretString) -> Self {
        Self {
            url: url.into(),
            token,
        }
    }
}

/// One workspace of a deployment, and optionally one of its agents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoderWorkspaceRef {
    /// The login name of the user the workspace belongs to.
    pub owner: String,
    /// The workspace's name, unique among its owner's.
    pub name: String,
    /// The agent to reach, for a workspace with more than one; `None` leaves the choice to the
    /// CLI, which is right for a workspace with exactly one.
    pub agent: Option<String>,
}

impl CoderWorkspaceRef {
    /// `owner`'s workspace `name`, on whichever agent it has.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
            agent: None,
        }
    }

    /// This workspace, on its agent `agent`.
    pub fn on_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }
}

/// The workspace as `coder ssh` is given it: `<owner>/<name>`, then `.<agent>` when one is named.
impl fmt::Display for CoderWorkspaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)?;
        if let Some(agent) = &self.agent {
            write!(f, ".{agent}")?;
        }
        Ok(())
    }
}

/// A streamed coder command's stdin: shutting it down drops the pipe, which is the command's EOF.
struct CoderStdin {
    inner: Option<tokio::process::ChildStdin>,
}

impl tokio::io::AsyncWrite for CoderStdin {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let Some(inner) = self.inner.as_mut() else {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the streamed command's stdin is closed",
            )));
        };
        std::pin::Pin::new(inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.inner.as_mut() {
            Some(inner) => std::pin::Pin::new(inner).poll_flush(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.inner = None;
        std::task::Poll::Ready(Ok(()))
    }
}

/// A shell on a Coder workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderShell {
    coder: PathBuf,
    ssh: PathBuf,
    credentials: CoderCredentials,
    workspace: CoderWorkspaceRef,
}

impl CoderShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "coder";

    /// The host name `ssh` is given: the `ProxyCommand` reaches the workspace, so it names nothing.
    const SSH_HOST_TOKEN: &'static str = "coder-workspace";

    /// What `coder ssh` exits with when the workspace's command failed, whatever its status was.
    pub const FAILED_COMMAND_EXIT_CODE: i32 = 1;

    /// What `coder ssh` prints, last, right before a failed command's status:
    /// `error: run command: Process exited with status 3`.
    pub const FAILED_COMMAND_STATUS: &'static str = "Process exited with status ";

    /// The variable the `coder` CLI reads its deployment's URL from.
    pub const URL_VARIABLE: &'static str = "CODER_URL";

    /// The variable the `coder` CLI reads its session token from.
    pub const SESSION_TOKEN_VARIABLE: &'static str = "CODER_SESSION_TOKEN";

    /// The shell on `workspace`, reached through the `coder` on `PATH` as `credentials`.
    pub fn new(credentials: CoderCredentials, workspace: CoderWorkspaceRef) -> Self {
        Self::through("coder", credentials, workspace)
    }

    /// [`Self::new`], reaching the workspace through the CLI `coder_binary`.
    pub fn through(
        coder_binary: impl Into<PathBuf>,
        credentials: CoderCredentials,
        workspace: CoderWorkspaceRef,
    ) -> Self {
        Self {
            coder: coder_binary.into(),
            ssh: PathBuf::from("ssh"),
            credentials,
            workspace,
        }
    }

    /// This shell, streaming its commands through the OpenSSH client `ssh_binary` rather than the
    /// `ssh` on `PATH`.
    pub fn with_ssh_binary(mut self, ssh_binary: impl Into<PathBuf>) -> Self {
        self.ssh = ssh_binary.into();
        self
    }

    /// The `coder` CLI this shell runs.
    pub fn coder_binary(&self) -> &Path {
        &self.coder
    }

    /// The deployment and token this shell presents.
    pub fn credentials(&self) -> &CoderCredentials {
        &self.credentials
    }

    /// The workspace this shell reaches.
    pub fn workspace(&self) -> &CoderWorkspaceRef {
        &self.workspace
    }

    /// The two variables as the interactive command's own environment.
    fn credential_env(&self) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        vec![
            (
                std::ffi::OsString::from(Self::URL_VARIABLE),
                std::ffi::OsString::from(&self.credentials.url),
            ),
            (
                std::ffi::OsString::from(Self::SESSION_TOKEN_VARIABLE),
                std::ffi::OsString::from(self.credentials.token.expose()),
            ),
        ]
    }

    /// `coder ssh --disable-autostart <workspace> -- sh -c '<remote_command>'`, with the two
    /// credential variables set on this child alone. The command is single-quoted as one
    /// argument: the CLI joins what follows `--` for the workspace's shell, and unquoted it
    /// would arrive as `sh -c`'s separate words rather than its one command string.
    fn command(&self, remote_command: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.coder);
        command
            .arg("ssh")
            .arg("--disable-autostart")
            .arg(self.workspace.to_string())
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(crate::shell_single_quote(remote_command))
            .env(Self::URL_VARIABLE, &self.credentials.url)
            .env(
                Self::SESSION_TOKEN_VARIABLE,
                self.credentials.token.expose(),
            )
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        command
    }

    /// The `ProxyCommand` that makes OpenSSH talk to the workspace through `coder ssh --stdio`,
    /// the CLI's plain-pipe transport. The coder path and the workspace are shell-quoted: OpenSSH
    /// hands the value to the user's shell.
    fn proxy_command(&self) -> String {
        format!(
            "{} ssh --stdio --disable-autostart {}",
            crate::shell_single_quote(&self.coder.to_string_lossy()),
            crate::shell_single_quote(&self.workspace.to_string()),
        )
    }

    /// `ssh <options> -o ProxyCommand=<proxy> -o StrictHostKeyChecking=no
    /// -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -- <host token> <command>`, with the two
    /// credential variables on this child's environment alone: the proxy inherits them, and neither
    /// is on any command line. The workspace is authenticated by the CLI's session token, not by a
    /// host key, so no host key is kept and none is asked about. `LogLevel=ERROR` keeps `ssh`'s own
    /// "Permanently added ... to the list of known hosts" warning out of the command's stderr.
    fn ssh_command(&self, remote_command: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.ssh);
        command
            .args(crate::SshShell::ssh_options())
            .arg("-o")
            .arg(format!("ProxyCommand={}", self.proxy_command()))
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
                "--",
                Self::SSH_HOST_TOKEN,
            ])
            .arg(remote_command)
            .env(Self::URL_VARIABLE, &self.credentials.url)
            .env(
                Self::SESSION_TOKEN_VARIABLE,
                self.credentials.token.expose(),
            )
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        command
    }

    fn spawn_error(&self, error: std::io::Error) -> RemoteShellError {
        RemoteShellError::Spawn(format!("{}: {error}", self.coder.display()))
    }

    /// The child's three pipes, or the spawn failure that says which one it has not got.
    fn take_pipes(
        child: &mut tokio::process::Child,
    ) -> Result<
        (
            tokio::process::ChildStdin,
            tokio::process::ChildStdout,
            tokio::process::ChildStderr,
        ),
        RemoteShellError,
    > {
        Ok((
            child
                .stdin
                .take()
                .ok_or_else(|| RemoteShellError::Spawn("coder child has no stdin".to_string()))?,
            child
                .stdout
                .take()
                .ok_or_else(|| RemoteShellError::Spawn("coder child has no stdout".to_string()))?,
            child
                .stderr
                .take()
                .ok_or_else(|| RemoteShellError::Spawn("coder child has no stderr".to_string()))?,
        ))
    }

    /// A finished one-shot command's output, or the workspace's unreachability. The start
    /// marker is what tells them apart: present means the command ran and the status is its
    /// own, so the marker is removed from the stream it arrived on and the rest is the
    /// command's output; absent with a non-zero status means nothing ran, and what the CLI
    /// said is the reason.
    ///
    /// The marker is printed to stderr, but `coder ssh` runs the command in a pty, which
    /// merges the command's stderr into its stdout — so it is recognised on either stream,
    /// with the `\r\n` a pty writes. Without it, a stopped workspace and a command that
    /// exits 1 are indistinguishable: `coder ssh` exits 1 for both.
    ///
    /// A marker on stdout proves the command ran in a merging pty, so what the CLI wrote on its
    /// own stderr is the transport's, not the command's — for a failed command, only its generic
    /// wrapper ("Encountered an error running \"coder ssh\" …"). The command's stderr is inside
    /// the merged stdout and cannot be told apart from its stdout. A command that failed there
    /// is therefore reported with the merged words as its stderr and nothing as its stdout, so
    /// every reader of a failure's stderr says what the command said, never the CLI's wrapper;
    /// a command that succeeded keeps the merged words as its stdout.
    fn output_of(
        &self,
        code: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Result<ExecOutput, RemoteShellError> {
        if let Some(stripped) = strip_start_marker(&stderr) {
            return Ok(ExecOutput {
                status: code,
                stdout,
                stderr: stripped.to_vec(),
            });
        }
        if let Some(stripped) = strip_start_marker(&stdout) {
            return Ok(if code == 0 {
                ExecOutput {
                    status: code,
                    stdout: stripped.to_vec(),
                    stderr,
                }
            } else {
                ExecOutput::exited(code, stripped.to_vec())
            });
        }
        if code == 0 {
            return Ok(ExecOutput {
                status: code,
                stdout,
                stderr,
            });
        }
        let detail = String::from_utf8_lossy(&stderr).trim().to_string();
        Err(RemoteShellError::Unreachable {
            address: self.address(),
            detail: if detail.is_empty() {
                String::from_utf8_lossy(&stdout).trim().to_string()
            } else {
                detail
            },
        })
    }
}

#[async_trait]
impl RemoteShell for CoderShell {
    /// The transport's tag and the workspace as `coder ssh` names it.
    fn address(&self) -> ShellAddress {
        ShellAddress::new(Self::TRANSPORT, self.workspace.to_string())
    }

    /// `coder ssh --disable-autostart <workspace> -- sh -c '<command>'`, with
    /// [`Self::URL_VARIABLE`] and [`Self::SESSION_TOKEN_VARIABLE`] set on that child alone. A
    /// `coder` that exits non-zero without having run the command — a workspace that is stopped,
    /// an agent that is not connected, a token the deployment refuses — is
    /// [`RemoteShellError::Unreachable`] with what it wrote to stderr.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        use tokio::io::AsyncWriteExt as _;

        let deadline = request.deadline;
        let lines = request.stdin.as_deref().map(stdin_lines);
        let wrapped = one_shot_command(
            &request.command,
            lines
                .as_ref()
                .zip(request.stdin.as_ref())
                .map(|(lines, bytes)| (lines.len(), bytes.len())),
            deadline,
        );
        let mut child = self
            .command(&wrapped)
            .spawn()
            .map_err(|error| self.spawn_error(error))?;
        let (mut stdin, mut stdout, mut stderr) = Self::take_pipes(&mut child)?;
        let coder_path = self.coder.clone();
        let work = async move {
            let marker_arrived = tokio::sync::Notify::new();
            let outputs = async {
                tokio::join!(
                    child.wait(),
                    read_noting_the_marker(&mut stdout, &marker_arrived),
                    read_noting_the_marker(&mut stderr, &marker_arrived),
                )
            };
            // The lines go only to a command that has turned its pty's echo off, which it has
            // once its marker has arrived; a call that ends without one never sends them.
            let feed = async {
                if let Some(lines) = lines {
                    marker_arrived.notified().await;
                    let _ = stdin.write_all(&lines).await;
                    let _ = stdin.flush().await;
                }
                drop(stdin);
            };
            tokio::pin!(outputs);
            tokio::select! {
                finished = &mut outputs => finished,
                () = feed => outputs.await,
            }
        };
        let (status, stdout, stderr) = match tokio::time::timeout(deadline, work).await {
            Ok(result) => result,
            Err(_) => {
                return Err(RemoteShellError::TimedOut {
                    address: self.address(),
                    after: deadline,
                    command: crate::redact::command_head(&request.command),
                });
            }
        };
        let status = status.map_err(|error| {
            RemoteShellError::Spawn(format!("{}: {error}", coder_path.display()))
        })?;
        self.output_of(status.code().unwrap_or(-1), stdout, stderr)
    }

    /// The command over OpenSSH, through `coder ssh --stdio` as its `ProxyCommand`: plain pipes,
    /// so the bytes are the command's own, its stderr is separate and its stdin's end is an EOF.
    /// `coder ssh` without `--stdio` runs a pty, which merges stderr, never ends the input and
    /// rewrites line endings.
    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        let mut child = self
            .ssh_command(command)
            .spawn()
            .map_err(|error| RemoteShellError::Spawn(format!("{}: {error}", self.ssh.display())))?;
        let (stdin, stdout, stderr) = Self::take_pipes(&mut child)?;
        let ssh_path = self.ssh.clone();
        Ok(StreamedExec {
            stdin: Box::new(CoderStdin { inner: Some(stdin) }),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            exit: crate::StreamedExit::new(async move {
                child
                    .wait()
                    .await
                    .map(|status| status.code().unwrap_or(-1))
                    .map_err(|error| {
                        RemoteShellError::Spawn(format!("{}: {error}", ssh_path.display()))
                    })
            }),
        })
    }

    /// `coder ssh --disable-autostart <workspace> -- <command>` (no `-t`: v2.33.5 refuses it, and
    /// the CLI gives the command the caller's PTY), the two variables in the command's `env`.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        InteractiveCommand {
            argv: vec![
                self.coder.as_os_str().to_os_string(),
                "ssh".into(),
                "--disable-autostart".into(),
                self.workspace.to_string().into(),
                "--".into(),
                command.into(),
            ],
            env: self.credential_env(),
            exit: InteractiveExit::ReportedLast {
                client_code: Self::FAILED_COMMAND_EXIT_CODE,
                marker: Self::FAILED_COMMAND_STATUS,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coder::one_shot::START_MARKER;
    use pretty_assertions::assert_eq;

    const ADAS_TOKEN: &str = "adas-coder-session-token";

    fn adas_credentials() -> CoderCredentials {
        CoderCredentials::new("https://coder.example.com", SecretString::new(ADAS_TOKEN))
    }

    #[test]
    fn a_workspace_reads_as_its_owner_and_name() {
        // Given
        let workspace = CoderWorkspaceRef::new("ada", "devbox");

        // When
        let shown = workspace.to_string();

        // Then
        assert_eq!(shown, "ada/devbox");
    }

    #[test]
    fn a_workspace_on_a_named_agent_reads_with_the_agent_after_a_dot() {
        // Given
        let workspace = CoderWorkspaceRef::new("ada", "devbox").on_agent("main");

        // When
        let shown = workspace.to_string();

        // Then
        assert_eq!(shown, "ada/devbox.main");
    }

    #[test]
    fn a_coder_shell_is_addressed_by_its_workspace() {
        // Given
        let shell = CoderShell::through(
            "/opt/bin/coder",
            adas_credentials(),
            CoderWorkspaceRef::new("ada", "devbox").on_agent("main"),
        );

        // When
        let address = shell.address();

        // Then
        assert_eq!(address, ShellAddress::new("coder", "ada/devbox.main"));
        assert_eq!(shell.coder_binary(), Path::new("/opt/bin/coder"));
    }

    #[test]
    fn a_marker_merged_into_stdout_with_a_ptys_line_ending_is_stripped_from_stdout() {
        // Given a shell whose command ran in a pty: the marker and the command's value
        // arrive on stdout together, the marker line ending in a carriage return
        let shell = CoderShell::through(
            "/opt/bin/coder",
            adas_credentials(),
            CoderWorkspaceRef::new("ada", "devbox"),
        );

        // When the finished command's output is read
        let output = shell.output_of(
            0,
            format!("{START_MARKER}\r\n/home/coder").into_bytes(),
            Vec::new(),
        );

        // Then the status is the command's and the value alone is its stdout
        assert_eq!(output, Ok(ExecOutput::succeeded("/home/coder")));
    }

    #[test]
    fn an_unreachable_workspace_says_what_the_cli_printed_on_either_stream() {
        // Given a shell the CLI refused to connect, printing why on stdout alone
        let shell = CoderShell::through(
            "/opt/bin/coder",
            adas_credentials(),
            CoderWorkspaceRef::new("ada", "devbox"),
        );

        // When the finished command's output is read
        let output = shell.output_of(1, b"workspace must be started\n".to_vec(), Vec::new());

        // Then the workspace is unreachable, with what the CLI said
        assert_eq!(
            output,
            Err(RemoteShellError::Unreachable {
                address: ShellAddress::new("coder", "ada/devbox"),
                detail: "workspace must be started".to_string(),
            })
        );
    }

    #[test]
    fn a_coder_shell_never_shows_its_token() {
        // Given
        let shell = CoderShell::new(adas_credentials(), CoderWorkspaceRef::new("ada", "devbox"));

        // When
        let shown = format!("{shell:?}");

        // Then
        assert_eq!(shown.contains(ADAS_TOKEN), false, "got {shown}");
        assert_eq!(
            shown.contains("https://coder.example.com"),
            true,
            "got {shown}"
        );
    }
}
