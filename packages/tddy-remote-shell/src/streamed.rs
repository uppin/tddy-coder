//! A long-running remote command with its three pipes open.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::RemoteShellError;

/// The writable end of a streamed command's standard input. Shutting it down is the command's EOF.
pub type ShellStdin = Box<dyn AsyncWrite + Send + Unpin>;
/// The readable end of a streamed command's standard output or standard error.
pub type ShellOutput = Box<dyn AsyncRead + Send + Unpin>;

/// A command started by [`crate::RemoteShell::exec_streamed`].
///
/// The pipes are transport-neutral byte streams rather than a local child's own handles, so a
/// transport that is not a local process — and the in-memory double — can produce one. Every
/// field is public so a caller can serve each pipe from a task of its own. Dropping all of it
/// stops the command.
pub struct StreamedExec {
    pub stdin: ShellStdin,
    pub stdout: ShellOutput,
    pub stderr: ShellOutput,
    pub exit: StreamedExit,
}

impl fmt::Debug for StreamedExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamedExec").finish_non_exhaustive()
    }
}

/// The end of a streamed command.
pub struct StreamedExit(Pin<Box<dyn Future<Output = Result<i32, RemoteShellError>> + Send>>);

impl StreamedExit {
    /// An exit that is known once `exit` resolves. A transport owns whatever keeps the command
    /// alive inside that future, so dropping the [`StreamedExit`] lets go of it.
    pub fn new(exit: impl Future<Output = Result<i32, RemoteShellError>> + Send + 'static) -> Self {
        Self(Box::pin(exit))
    }

    /// The command's exit status, once it has exited.
    pub async fn wait(self) -> Result<i32, RemoteShellError> {
        self.0.await
    }
}

impl fmt::Debug for StreamedExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamedExit").finish_non_exhaustive()
    }
}
