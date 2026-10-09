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

mod deferred;
mod jsonl;
mod redact;

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio,
};
use std::sync::Arc;

use tddy_lsp::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

pub use deferred::ColdRunSpawnRecord;
pub use jsonl::JsonlSpawnRecord;
pub use redact::redacted;

/// The short constants a run names its processes by.
///
/// A purpose travels with a record so a reader can tell one `cargo` from another without reading
/// its argv. The values are the wire's: `run-index-daemon` writes `build-daemon`,
/// `capture-dev-shell-env` and `index-daemon` itself, and the JSONL sink reads
/// [`LANGUAGE_SERVER`](purpose::LANGUAGE_SERVER) to label a record `lsp` rather than `engine`.
pub mod purpose {
    /// The cold command line's self-started rust-analyzer.
    pub const RUST_ANALYZER: &str = "rust-analyzer";
    /// A `cargo check` — the baseline, the result check, the group gate or the tidy's own.
    pub const COMPILE_GATE: &str = "compile-gate";
    /// One `rustfmt` per file the tidy formats.
    pub const TIDY_FORMAT: &str = "tidy-format";
    /// One `git` invocation.
    pub const GIT: &str = "git";
    /// The language server a host (`tddy-lsp`) starts for a warm index.
    pub const LANGUAGE_SERVER: &str = "language-server";
}

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

    /// A recorder that labels every process it starts as started for plan operation `op`, so its
    /// spawn record line names the operation the journal records it under.
    ///
    /// What `commit_operation` starts its `git` processes through.
    pub fn for_operation(&self, op: usize, op_id: Option<&crate::OpId>) -> SpawnRecorder {
        let _ = (op, op_id);
        todo!("TODO(reshape-apply-robust): implement — carry an OperationContext with op and op_id")
    }

    /// A recorder that labels every process it starts as started for the transactional group
    /// `group` — what a group's end-of-group compile gate runs through.
    pub fn for_group(&self, group: &str) -> SpawnRecorder {
        let _ = group;
        todo!("TODO(reshape-apply-robust): implement — carry an OperationContext with the group")
    }

    /// Whether anyone is told about the processes this recorder starts.
    pub fn is_listened_to(&self) -> bool {
        self.observer.is_some()
    }

    /// A `Command` for `program`, to be run through this recorder.
    ///
    /// The only way this crate builds a `Command`: a process started any other way would be a
    /// process the run cannot account for, which is what the structural guard exists to prevent.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        Command::new(program)
    }

    /// Run `command` to completion and return what it wrote, reporting it as `purpose`.
    ///
    /// Both streams are captured, as [`Command::output`] captures them, so the caller reads the
    /// same [`Output`] it would from there.
    pub fn output(&self, purpose: &'static str, command: &mut Command) -> io::Result<Output> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = self.spawn(purpose, command)?;
        let stdout = drain(child.take_stdout());
        let stderr = drain(child.take_stderr());
        let status = child.wait()?;
        Ok(Output {
            status,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }

    /// Start `command` and hand back a child whose end is reported when it is waited on,
    /// reporting its start as `purpose`.
    ///
    /// A spawn that fails is reported as a start with no pid and then an end carrying the error,
    /// before the error is handed back.
    pub fn spawn(&self, purpose: &'static str, command: &mut Command) -> io::Result<RecordedChild> {
        match command.spawn() {
            Ok(child) => {
                let start = process_start(purpose, command, Some(child.id()));
                let token = self
                    .observer
                    .as_ref()
                    .map(|observer| observer.started(&start));
                Ok(RecordedChild {
                    child,
                    observer: self.observer.clone(),
                    token,
                    ended: false,
                })
            }
            Err(error) => {
                if let Some(observer) = &self.observer {
                    let start = process_start(purpose, command, None);
                    let token = observer.started(&start);
                    observer.ended(
                        token,
                        &ProcessOutcome::SpawnFailed {
                            error: error.to_string(),
                        },
                    );
                }
                Err(error)
            }
        }
    }
}

/// What a `Command` says about itself, so an observer can be told without running anything.
fn process_start(purpose: &'static str, command: &Command, pid: Option<u32>) -> ProcessStart {
    ProcessStart {
        purpose,
        program: command.get_program().to_string_lossy().into_owned(),
        args: command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        cwd: command.get_current_dir().map(Path::to_path_buf),
        env_names: command
            .get_envs()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect(),
        pid,
        operation: None,
    }
}

/// Everything a child's pipe carries, read to the end on a thread of its own.
fn drain<R: io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// How a process ended, from the status it was waited on with.
fn outcome_of(status: ExitStatus) -> ProcessOutcome {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ProcessOutcome::Signalled { signal };
        }
    }
    ProcessOutcome::Exited {
        code: status.code().unwrap_or(-1),
    }
}

/// A child started through a [`SpawnRecorder`]. Waiting on it, or killing it, is what ends its
/// record.
pub struct RecordedChild {
    child: Child,
    observer: Option<Arc<dyn SpawnObserver>>,
    token: Option<ProcessToken>,
    /// Whether the end has been reported, so a `try_wait` poll and a later `wait` report it once.
    ended: bool,
}

impl RecordedChild {
    /// The process id.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Whether the child has exited, without waiting for it. An exit seen here ends its record.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if let Some(status) = status {
            self.report(status);
        }
        Ok(status)
    }

    /// Wait for the child to exit. This is what ends its record.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait()?;
        self.report(status);
        Ok(status)
    }

    /// Kill the child. Its end is reported by the `wait` that follows.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// The child's piped standard input, once.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    /// The child's piped standard output, once.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// The child's piped standard error, once.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// Tell the observer how the child ended, once.
    fn report(&mut self, status: ExitStatus) {
        if self.ended {
            return;
        }
        self.ended = true;
        if let (Some(observer), Some(token)) = (&self.observer, self.token) {
            observer.ended(token, &outcome_of(status));
        }
    }
}
