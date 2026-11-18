//! A remote shell that reports every call it makes: what the start of a session on a target did
//! there, as a log (`#coder-start` 8/10).
//!
//! [`RecordedShell`] wraps any [`RemoteShell`] and tells a [`RemoteCallSink`] about each
//! [`RemoteShell::exec`] when it ends: the target, the caller's command (truncated, credentials
//! scrubbed), how long it took, and how it ended. The call itself is unchanged.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, RemoteShell, RemoteShellError, ShellAddress,
    StreamedExec,
};

/// How a recorded call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCallOutcome {
    /// The command ran and exited with `code`.
    Exited { code: i32 },
    /// The command was still running at its deadline and was stopped.
    TimedOut,
    /// The transport could not reach the target; nothing ran.
    Unreachable,
}

/// One finished call, as a [`RemoteCallSink`] is told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCallReport {
    pub address: ShellAddress,
    /// The caller's own command (never a transport's wrapper), truncated, credentials scrubbed.
    pub command: String,
    pub duration: Duration,
    pub outcome: RemoteCallOutcome,
    /// The tail of the command's stderr when the call failed; empty for a call that exited zero.
    pub stderr_tail: String,
}

/// Where a [`RecordedShell`] reports its calls.
pub trait RemoteCallSink: Send + Sync {
    fn report(&self, call: RemoteCallReport);
}

/// `inner`, reporting each of its calls to `sink`.
pub struct RecordedShell {
    inner: Arc<dyn RemoteShell>,
    // TODO(start-visibility): implement - read once `exec` reports.
    #[allow(dead_code)]
    sink: Arc<dyn RemoteCallSink>,
}

impl RecordedShell {
    pub fn new(inner: Arc<dyn RemoteShell>, sink: Arc<dyn RemoteCallSink>) -> Self {
        Self { inner, sink }
    }
}

#[async_trait]
impl RemoteShell for RecordedShell {
    fn address(&self) -> ShellAddress {
        self.inner.address()
    }

    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        // TODO(start-visibility): implement - time the call and report it to the sink.
        self.inner.exec(request).await
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        self.inner.exec_streamed(command).await
    }

    fn interactive(&self, command: &str) -> InteractiveCommand {
        self.inner.interactive(command)
    }
}
