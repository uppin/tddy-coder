//! OpenSSH as a remote-shell transport: the `ssh` client on this host, so `~/.ssh/config` and
//! ssh-agent apply. `BatchMode=yes` always — an unloaded key is a hard failure, never a prompt.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::process::{ChildStdin, Command};

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, RemoteShell, RemoteShellError, ShellAddress,
    StreamedExec, StreamedExit,
};

/// A streamed ssh command's stdin: shutting it down drops the pipe, which is the command's EOF
/// (a tokio `ChildStdin`'s own shutdown does not close it).
struct SshStdin {
    inner: Option<ChildStdin>,
}

impl AsyncWrite for SshStdin {
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

/// A shell on the ssh `Host` alias `alias`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshShell {
    ssh: PathBuf,
    alias: String,
}

impl SshShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "ssh";

    /// The shell on `alias`, reached through the `ssh` on `PATH`.
    pub fn new(alias: impl Into<String>) -> Self {
        Self::through("ssh", alias)
    }

    /// [`Self::new`], reaching the target through the ssh client `ssh_binary`.
    pub fn through(ssh_binary: impl Into<PathBuf>, alias: impl Into<String>) -> Self {
        Self {
            ssh: ssh_binary.into(),
            alias: alias.into(),
        }
    }

    /// The ssh client this shell runs.
    pub fn ssh_binary(&self) -> &Path {
        &self.ssh
    }

    /// The `Host` alias this shell reaches.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// The connection options every ssh this crate starts carries, as `-o <option>` pairs: no
    /// prompt (`BatchMode=yes`), and a connection that cannot hang (`ConnectTimeout=15`,
    /// `ServerAliveInterval=5`, `ServerAliveCountMax=3`).
    pub fn ssh_options() -> Vec<String> {
        [
            "BatchMode=yes",
            "ConnectTimeout=15",
            "ServerAliveInterval=5",
            "ServerAliveCountMax=3",
        ]
        .into_iter()
        .flat_map(|option| ["-o".to_string(), option.to_string()])
        .collect()
    }

    fn command(&self, remote_command: &str) -> Command {
        let mut command = Command::new(&self.ssh);
        command
            .args(Self::ssh_options())
            .arg("--")
            .arg(&self.alias)
            .arg(remote_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    fn spawn_error(&self, error: std::io::Error) -> RemoteShellError {
        RemoteShellError::Spawn(format!("{}: {error}", self.ssh.display()))
    }
}

#[async_trait]
impl RemoteShell for SshShell {
    fn address(&self) -> ShellAddress {
        ShellAddress::new(Self::TRANSPORT, self.alias.clone())
    }

    /// `ssh <options> -- <alias> <command>`. OpenSSH's own exit status 255 is
    /// [`RemoteShellError::Unreachable`] with what it wrote to stderr.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        let deadline = request.deadline;
        let mut child = self
            .command(&request.command)
            .spawn()
            .map_err(|error| self.spawn_error(error))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stdin".to_string()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stdout".to_string()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stderr".to_string()))?;
        let stdin_bytes = request.stdin;
        let ssh_path = self.ssh.clone();
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
                    command: crate::redact::command_head(&request.command),
                });
            }
        };
        let status = status
            .map_err(|error| RemoteShellError::Spawn(format!("{}: {error}", ssh_path.display())))?;
        let code = status.code().unwrap_or(-1);
        if code == 255 {
            return Err(RemoteShellError::Unreachable {
                address: self.address(),
                detail: String::from_utf8_lossy(&stderr).trim().to_string(),
            });
        }
        Ok(ExecOutput {
            status: code,
            stdout,
            stderr,
        })
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        let mut child = self
            .command(command)
            .spawn()
            .map_err(|error| self.spawn_error(error))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RemoteShellError::Spawn("ssh child has no stderr".to_string()))?;
        let ssh_path = self.ssh.clone();
        Ok(StreamedExec {
            stdin: Box::new(SshStdin { inner: Some(stdin) }),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            exit: StreamedExit::new(async move {
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

    /// `ssh -tt <options> -- <alias> <command>`.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        let mut argv: Vec<OsString> = vec![self.ssh.as_os_str().to_os_string(), "-tt".into()];
        argv.extend(Self::ssh_options().into_iter().map(OsString::from));
        argv.push("--".into());
        argv.push(self.alias.clone().into());
        argv.push(command.into());
        InteractiveCommand {
            argv,
            env: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn the_connection_options_never_prompt_and_cannot_hang() {
        // Given / When
        let options = SshShell::ssh_options();

        // Then
        assert_eq!(
            options,
            [
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=15",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=3",
            ]
        );
    }

    #[test]
    fn an_ssh_shell_is_addressed_by_its_alias() {
        // Given
        let shell = SshShell::through("/opt/bin/ssh", "buildbox");

        // When
        let address = shell.address();

        // Then
        assert_eq!(address, ShellAddress::new("ssh", "buildbox"));
        assert_eq!(shell.ssh_binary(), Path::new("/opt/bin/ssh"));
        assert_eq!(shell.alias(), "buildbox");
    }
}
