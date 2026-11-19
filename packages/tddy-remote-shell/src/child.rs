//! A transport whose client is a local child process (`ssh`, `sh`): running one command to its end
//! under a deadline, or streaming one, over the child's three pipes.

use std::path::Path;

use tokio::io::{AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::process::{ChildStdin, Command};

use crate::{RemoteShellError, ShellAddress, StreamedExec, StreamedExit};

/// A streamed command's stdin: shutting it down drops the pipe, which is the command's EOF
/// (a tokio `ChildStdin`'s own shutdown does not close it).
struct ClosableStdin {
    inner: Option<ChildStdin>,
}

impl AsyncWrite for ClosableStdin {
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

/// What a child that ran one command to its end left: its exit code (`-1` for a signal) and both
/// streams, whole.
pub(crate) struct ChildOutput {
    pub(crate) code: i32,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

/// Run `command` — its three pipes set up, killed on drop — with `stdin` written then closed, and
/// wait for it within `deadline`. A child past the deadline is dropped, which kills it, and is
/// `TimedOut` on `address`, naming `request_command`.
pub(crate) async fn run_to_end(
    mut command: Command,
    program: &Path,
    stdin: Option<Vec<u8>>,
    deadline: std::time::Duration,
    address: ShellAddress,
    request_command: &str,
) -> Result<ChildOutput, RemoteShellError> {
    let spawn_error =
        |error: std::io::Error| RemoteShellError::Spawn(format!("{}: {error}", program.display()));
    let mut child = command.spawn().map_err(spawn_error)?;
    let (mut child_stdin, mut stdout, mut stderr) = pipes_of(&mut child, program)?;
    let work = async move {
        let stdin_fut = async {
            if let Some(bytes) = stdin {
                let _ = child_stdin.write_all(&bytes).await;
            }
            drop(child_stdin);
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
                address,
                after: deadline,
                command: crate::redact::command_head(request_command),
            });
        }
    };
    let status = status.map_err(spawn_error)?;
    Ok(ChildOutput {
        code: status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

/// Start `command` — its three pipes set up, killed on drop — as a streamed command: its pipes and
/// its exit, which is the child's code (`-1` for a signal).
pub(crate) fn stream(
    mut command: Command,
    program: &Path,
) -> Result<StreamedExec, RemoteShellError> {
    let mut child = command
        .spawn()
        .map_err(|error| RemoteShellError::Spawn(format!("{}: {error}", program.display())))?;
    let (stdin, stdout, stderr) = pipes_of(&mut child, program)?;
    let program = program.to_path_buf();
    Ok(StreamedExec {
        stdin: Box::new(ClosableStdin { inner: Some(stdin) }),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        exit: StreamedExit::new(async move {
            child
                .wait()
                .await
                .map(|status| status.code().unwrap_or(-1))
                .map_err(|error| RemoteShellError::Spawn(format!("{}: {error}", program.display())))
        }),
    })
}

fn pipes_of(
    child: &mut tokio::process::Child,
    program: &Path,
) -> Result<
    (
        ChildStdin,
        tokio::process::ChildStdout,
        tokio::process::ChildStderr,
    ),
    RemoteShellError,
> {
    let missing =
        |pipe: &str| RemoteShellError::Spawn(format!("{} child has no {pipe}", program.display()));
    Ok((
        child.stdin.take().ok_or_else(|| missing("stdin"))?,
        child.stdout.take().ok_or_else(|| missing("stdout"))?,
        child.stderr.take().ok_or_else(|| missing("stderr"))?,
    ))
}
