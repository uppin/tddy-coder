//! The seam through which a host learns which processes a crate starts, and how each one ended.
//!
//! This crate starts one process, the language server, and the crates above it start more. The
//! trait lives here because `tddy-lsp` is the lowest crate both the engine and the index daemon
//! already depend on, so neither needs a new edge to report through it.
//!
//! The observer is handed **raw** arguments, in process. Nothing here persists them: whatever
//! writes a record decides what may reach a file.

use std::path::PathBuf;

/// What a host that wants to know which processes a crate starts implements.
pub trait SpawnObserver: Send + Sync {
    /// Called once the process exists (pid known), or with `pid: None` and then
    /// [`ProcessOutcome::SpawnFailed`] through [`SpawnObserver::ended`].
    fn started(&self, process: &ProcessStart) -> ProcessToken;

    /// Called once the process has been waited on, with how it ended.
    fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome);
}

/// A process that was started, or that could not be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessStart {
    /// A short constant naming why this process runs (`compile-gate`, `language-server`, ...).
    pub purpose: &'static str,
    /// The program as it was named, not as it was resolved.
    pub program: String,
    /// Every argument, unredacted.
    pub args: Vec<String>,
    /// The directory the process was started in, when one was set.
    pub cwd: Option<PathBuf>,
    /// The **names** of the environment variables set for this process. Never their values.
    pub env_names: Vec<String>,
    /// The process id, absent when the process could not be started.
    pub pid: Option<u32>,
    /// The unit of work above this crate that started the process — a restructure plan's
    /// operation or group — when it was started for one. `None` for run-level processes, and
    /// always for the language server this crate starts.
    pub operation: Option<OperationContext>,
}

/// Which unit of a caller's work a process was started for, so a record of it can be joined to
/// that caller's own record (a restructure journal names operations by index and id).
///
/// Opaque to this crate: it carries the labels and never reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationContext {
    /// The operation's index in its plan.
    pub op: Option<usize>,
    /// The operation's stable id, when the plan has one.
    pub op_id: Option<String>,
    /// The transactional group the process ran for (a group's compile gate).
    pub group: Option<String>,
}

/// What an observer hands back from [`SpawnObserver::started`], to pair the end with the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessToken(pub u64);

/// How a process ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// It exited of its own accord, or was told to and did.
    Exited { code: i32 },
    /// It was terminated by a signal and has no exit code.
    Signalled { signal: i32 },
    /// It never started.
    SpawnFailed { error: String },
}
