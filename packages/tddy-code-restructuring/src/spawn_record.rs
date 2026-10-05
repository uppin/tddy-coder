//! The one place this crate starts a process, so a run can say what it executed.
//!
//! A restructure run starts `git`, `cargo check`, `rustfmt` and, on the cold command line, a
//! language server. Every one of those goes through a [`SpawnRecorder`], which tells a
//! [`SpawnObserver`] when the process exists and how it ended. The default recorder tells nobody,
//! as `Options::progress` defaults to [`discard`](crate::backends::rust::discard): a library that
//! was not asked to report says nothing.
//!
//! The recorder wraps a `Command`; it never decides one. What a run starts, with which arguments
//! and in which order, is the caller's.

mod jsonl;
mod redact;

use std::io;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Output};
use std::sync::Arc;

use tddy_lsp::{ProcessToken, SpawnObserver};

pub use jsonl::JsonlSpawnRecord;
pub use redact::redacted;

/// Starts processes on behalf of a run, telling an observer about each one.
#[derive(Clone)]
pub struct SpawnRecorder {
    observer: Option<Arc<dyn SpawnObserver>>,
}

impl SpawnRecorder {
    /// A recorder nobody listens to: processes start exactly as they would without one.
    pub fn discard() -> Self {
        Self { observer: None }
    }

    /// A recorder that reports to `observer`.
    pub fn new(observer: Arc<dyn SpawnObserver>) -> Self {
        Self {
            observer: Some(observer),
        }
    }

    /// Whether anyone is told about the processes this recorder starts.
    pub fn is_listened_to(&self) -> bool {
        self.observer.is_some()
    }

    /// Run `command` to completion and return what it wrote, reporting it as `purpose`.
    ///
    /// TODO(spawn-record): implement. The process runs; the observer is not told yet.
    pub fn output(&self, purpose: &'static str, command: &mut Command) -> io::Result<Output> {
        let _ = purpose;
        command.output()
    }

    /// Start `command` and hand back a child whose end is reported when it is waited on,
    /// reporting its start as `purpose`.
    ///
    /// TODO(spawn-record): implement. The process starts; the observer is not told yet.
    pub fn spawn(&self, purpose: &'static str, command: &mut Command) -> io::Result<RecordedChild> {
        let _ = purpose;
        let child = command.spawn()?;
        Ok(RecordedChild { child, token: None })
    }
}

/// A child started through a [`SpawnRecorder`]. Waiting on it, or killing it, is what ends its
/// record.
pub struct RecordedChild {
    child: Child,
    // TODO(spawn-record): set from `SpawnObserver::started`, read when the child ends.
    #[allow(dead_code)]
    token: Option<ProcessToken>,
}

impl RecordedChild {
    /// The process id.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Whether the child has exited, without waiting for it.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Wait for the child to exit.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    /// Kill the child. Its end is reported by the `wait` that follows.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// The child's piped standard output, once.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// The child's piped standard error, once.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }
}
