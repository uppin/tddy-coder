//! Replays a JSONL plan of named refactoring intents against real language refactoring engines.
//!
//! The plan is a command log of *intents* — never code text. Each intent is resolved into a
//! multi-file [`WorkspaceEdit`] by a [`LanguageBackend`], applied to disk, and recorded in an
//! append-only event journal. The [`PositionLedger`] is a projection over that journal, which is
//! what lets a run resume after a crash.

pub mod apply;
pub mod backends;
pub mod console;
pub mod crate_move;
pub mod edit;
pub mod journal;
pub mod ledger;
pub mod overlay;
pub mod plan;
pub mod registry;
mod restructure_args;
pub mod restructure_cli;
pub mod runner;
pub mod verify;

pub use backends::rust::{client_capabilities, server_settings};
pub use crate_move::{
    defining_crate, module_home, read_test_binary_move, resolve_cluster, resolve_test_binary_move,
    siblings_left_behind, unrunnable_moves, CallerRewrite, Destination, ModuleHome, MovingCluster,
    Survey, TestBinaryMove,
};
pub use edit::{FileEdit, Position, Range, Resolution, TextEdit, VisibilityChange, WorkspaceEdit};
pub use journal::{Journal, JournalRecord, OpStatus};
pub use ledger::{LedgerCheckpoint, PositionLedger};
pub use overlay::Overlay;
pub use plan::{Anchor, Plan, Reexport, RefactorKind, RefactorOp};
pub use registry::{BackendRegistry, LanguageBackend};
pub use runner::state_directory_for_plan;

/// Errors surfaced by the executor. Every variant is fatal — the executor never falls back.
#[derive(Debug, thiserror::Error)]
pub enum RestructureError {
    #[error("plan is malformed: {0}")]
    MalformedPlan(String),
    /// The plan is well formed and the code will not permit this cut.
    ///
    /// Kept apart from [`RestructureError::MalformedPlan`] for the reason
    /// [`RestructureError::ServerNotSettled`] already states, applied to the other large family of
    /// refusals: a malformed plan is fixed by editing the plan, and a seam the code refuses is fixed
    /// by cutting it elsewhere or by changing the code. Stranded references, an `impl` cut in half,
    /// a module name already taken, an import the file's own bindings cannot disambiguate — none of
    /// them is a defect in the plan, and every one of them used to say it was.
    #[error("this seam cannot be cut here: {0}")]
    SeamRefused(String),
    /// rust-analyzer answered, and the answer could not be used.
    ///
    /// An extraction produced before the types were inferred, a rewrite that came back mangled, a
    /// response carrying no edits. The remedy is to retry against a warm server or to look at the
    /// server, and neither is something an author does to a plan.
    #[error("rust-analyzer's answer was unusable: {0}")]
    ServerDefect(String),
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
    /// A journal keyed by the repository rather than by a plan is standing over this plan's own.
    ///
    /// Run state is keyed by the plan ([`runner::state_directory_for_plan`]); a journal at
    /// `<root>/.restructure/` is whatever ran last under this root, and nothing in it says which
    /// plan that was. Adopting it for the plan in hand would let `--resume` replay another plan's
    /// operations against these coordinates, which is the one outcome worse than refusing.
    #[error(
        "{path} is a repository-scoped journal, which belongs to no plan this run can name — \
         archive or remove it, or pass --resume to continue it as this plan's own journal"
    )]
    RepoScopedJournal { path: String },
    #[error("the language server is still catching up with an earlier change")]
    ServerCatchingUp,
    /// The server stayed unable to answer one method, as distinct from the plan being wrong.
    ///
    /// Kept apart from [`RestructureError::MalformedPlan`] because a caller acts on the difference:
    /// a malformed plan is fixed by editing the plan, and a server that will not settle is fixed by
    /// waiting or by looking at the server. Reporting the second as the first is what makes the
    /// advice "fix your plan" actively misleading.
    #[error(
        "rust-analyzer would not settle enough to answer {method} after {seconds}s \
         (last progress: {last})"
    )]
    ServerNotSettled {
        method: String,
        seconds: u64,
        last: String,
    },
    /// The wait for the index ended before the server was ready, because its caller stopped
    /// waiting. Nothing else ends such a wait: there is no budget to raise, so the message names
    /// where the index got to instead of advising a number.
    #[error(
        "rust-analyzer had not finished indexing after {seconds}s (last progress: {last}) and the \
         wait was cancelled. Toolchain it resolved with: {environment}"
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
    /// The caller stopped waiting while a language-server request was in flight, so the request
    /// was abandoned — at the server too, which is told to stop computing an answer nobody will
    /// read.
    ///
    /// Distinct from every other variant because nothing is wrong: not the plan, not the tree, not
    /// the server. It is also the one refusal that must **not** be retried — there is nobody left
    /// to answer — which is why it is a variant of its own rather than folded into
    /// [`RestructureError::ServerCatchingUp`]. The backend converts it into
    /// [`RestructureError::IndexingIncomplete`] as it leaves, so the run still reports how far the
    /// index got; it is visible here for the paths that have no index to report on.
    #[error("the caller stopped waiting, so the request was abandoned")]
    CallerStopped,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RestructureError>;
