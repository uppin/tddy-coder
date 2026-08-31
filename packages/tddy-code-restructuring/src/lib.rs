//! Replays a JSONL plan of named refactoring intents against real language refactoring engines.
//!
//! The plan is a command log of *intents* — never code text. Each intent is resolved into a
//! multi-file [`WorkspaceEdit`] by a [`LanguageBackend`], applied to disk, and recorded in an
//! append-only event journal. The [`PositionLedger`] is a projection over that journal, which is
//! what lets a run resume after a crash.

pub mod apply;
pub mod backends;
pub mod edit;
pub mod journal;
pub mod ledger;
pub mod overlay;
pub mod plan;
pub mod registry;
pub mod runner;
pub mod verify;

pub use edit::{FileEdit, Position, Range, Resolution, TextEdit, VisibilityChange, WorkspaceEdit};
pub use journal::{Journal, JournalRecord, OpStatus};
pub use ledger::{LedgerCheckpoint, PositionLedger};
pub use overlay::Overlay;
pub use plan::{Anchor, Plan, Reexport, RefactorKind, RefactorOp};
pub use registry::{BackendRegistry, LanguageBackend};

/// Errors surfaced by the executor. Every variant is fatal — the executor never falls back.
#[derive(Debug, thiserror::Error)]
pub enum RestructureError {
    #[error("plan is malformed: {0}")]
    MalformedPlan(String),
    #[error("plan carries code text in field `{field}` — plans hold intents only")]
    CodeTextInPlan { field: String },
    #[error("snapshot mismatch for {path}: plan expected {expected}, working tree has {actual}")]
    SnapshotMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error(
        "anchor invalidated: position in {path} fell inside text removed by an earlier operation"
    )]
    AnchorInvalidated { path: String },
    #[error("no backend handles `{extension}`")]
    NoBackend { extension: String },
    #[error("backend `{backend}` does not support operation `{op}`")]
    UnsupportedOp { backend: String, op: String },
    #[error("{path} is not inside a git worktree — `git mv` is required to preserve history")]
    NotAGitWorktree { path: String },
    #[error("journal is indeterminate at op {op}: files match neither the pre- nor post-operation state")]
    IndeterminateJournal { op: usize },
    #[error("ledger checkpoint at op {op} disagrees with the journal it was derived from")]
    CheckpointDivergence { op: usize },
    #[error("a journal already exists for this plan — pass --resume to continue it")]
    JournalExists,
    #[error("the language server is still catching up with an earlier change")]
    ServerCatchingUp,
    #[error(
        "rust-analyzer had not finished indexing after {seconds}s (last progress: {last}) — \
         raise the budget with --indexing-budget <seconds>. Toolchain it resolved with: \
         {environment}"
    )]
    IndexingIncomplete {
        seconds: u64,
        last: String,
        /// What rust-analyzer was actually launched against. A stall at `discovering sysroot`
        /// looks identical whether the toolchain was pinned, whether cargo/rustc resolved to
        /// real binaries or to rustup proxies, and whether it was reaching the network — this
        /// is the line that separates them in a CI log.
        environment: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RestructureError>;
