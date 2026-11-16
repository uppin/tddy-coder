//! A Coder workspace as a remote-shell transport: the `coder` CLI on this host, given the
//! deployment's URL and a session token in its own environment and nowhere else.
//!
//! `coder ssh` is run with `--disable-autostart` always: this transport never starts a workspace.
//! One that is not running is [`RemoteShellError::Unreachable`], with what the CLI said.

use std::fmt;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_credentials::SecretString;

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, RemoteShell, RemoteShellError, ShellAddress,
    StreamedExec,
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

/// The line the one-shot wrapper prints to stderr before the command runs.
///
/// `coder ssh` exits 1 both when the workspace cannot be entered and when the command exits 1,
/// so the exit status alone cannot tell those apart. This marker can: present means the command
/// ran and its status is its own; absent with a non-zero exit means nothing ran.
const START_MARKER: &str = "tddy-coder-shell: the command is running";

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
    credentials: CoderCredentials,
    workspace: CoderWorkspaceRef,
}

impl CoderShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "coder";

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
            credentials,
            workspace,
        }
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
    /// marker on stderr is what tells them apart: present means the command ran and the status
    /// is its own, so the marker is removed and the rest is the command's stderr; absent with a
    /// non-zero status means nothing ran, and what the CLI said is the reason.
    fn output_of(
        &self,
        code: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Result<ExecOutput, RemoteShellError> {
        let marker_line = format!("{START_MARKER}\n");
        if let Some(stripped) = stderr.strip_prefix(marker_line.as_bytes()) {
            return Ok(ExecOutput {
                status: code,
                stdout,
                stderr: stripped.to_vec(),
            });
        }
        if code == 0 {
            return Ok(ExecOutput {
                status: code,
                stdout,
                stderr,
            });
        }
        Err(RemoteShellError::Unreachable {
            address: self.address(),
            detail: String::from_utf8_lossy(&stderr).trim().to_string(),
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
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let deadline = request.deadline;
        let wrapped = format!("printf '%s\\n' '{START_MARKER}' >&2; {}", request.command);
        let mut child = self
            .command(&wrapped)
            .spawn()
            .map_err(|error| self.spawn_error(error))?;
        let (mut stdin, mut stdout, mut stderr) = Self::take_pipes(&mut child)?;
        let stdin_bytes = request.stdin;
        let coder_path = self.coder.clone();
        let work = async move {
            let stdin_fut = async {
                if let Some(bytes) = stdin_bytes {
                    let _ = stdin.write_all(&bytes).await;
                }
                drop(stdin);
            };
            let stdout_fut = async {
                let mut bytes = Vec::new();
                let _ = stdout.read_to_end(&mut bytes).await;
                bytes
            };
            let stderr_fut = async {
                let mut bytes = Vec::new();
                let _ = stderr.read_to_end(&mut bytes).await;
                bytes
            };
            let status_fut = async { child.wait().await };
            let (status, (), stdout_bytes, stderr_bytes) =
                tokio::join!(status_fut, stdin_fut, stdout_fut, stderr_fut);
            (status, stdout_bytes, stderr_bytes)
        };
        let (status, stdout, stderr) = match tokio::time::timeout(deadline, work).await {
            Ok(result) => result,
            Err(_) => {
                return Err(RemoteShellError::TimedOut {
                    address: self.address(),
                    after: deadline,
                });
            }
        };
        let status = status.map_err(|error| {
            RemoteShellError::Spawn(format!("{}: {error}", coder_path.display()))
        })?;
        self.output_of(status.code().unwrap_or(-1), stdout, stderr)
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        let mut child = self
            .command(command)
            .spawn()
            .map_err(|error| self.spawn_error(error))?;
        let (stdin, stdout, stderr) = Self::take_pipes(&mut child)?;
        let coder_path = self.coder.clone();
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
                        RemoteShellError::Spawn(format!("{}: {error}", coder_path.display()))
                    })
            }),
        })
    }

    /// `coder ssh --disable-autostart -t <workspace> -- <command>`, with the two variables in the
    /// command's `env`: the caller's PTY child is the one process that is given them.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        InteractiveCommand {
            argv: vec![
                self.coder.as_os_str().to_os_string(),
                "ssh".into(),
                "--disable-autostart".into(),
                "-t".into(),
                self.workspace.to_string().into(),
                "--".into(),
                command.into(),
            ],
            env: self.credential_env(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
