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
pub mod item_anchor;
pub mod journal;
pub mod ledger;
pub mod overlay;
pub mod plan;
pub mod plan_store;
pub mod registry;
mod restructure_args;
pub mod restructure_cli;
pub mod runner;
pub mod spawn_record;
pub mod verify;

pub use backends::rust::{client_capabilities, server_settings};
pub use crate_move::{
    defining_crate, module_home, read_test_binary_move, resolve_cluster, resolve_test_binary_move,
    siblings_left_behind, unrunnable_moves, CallerRewrite, Destination, ModuleHome, MovingCluster,
    NewCrate, Survey, TestBinaryMove,
};
pub use edit::{FileEdit, Position, Range, Resolution, TextEdit, VisibilityChange, WorkspaceEdit};
pub use journal::{Journal, JournalRecord, OpStatus, OpenGroup, PreImage};
pub use ledger::{LedgerCheckpoint, PositionLedger};
pub use overlay::Overlay;
pub use plan::{
    Anchor, FileHint, Fingerprint, ItemPath, ItemSegment, OpId, OrderKey, Plan, Reexport,
    RefactorKind, RefactorOp,
};
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
    /// The item an anchor names is no longer the text the anchor was written against.
    ///
    /// An edit *outside* an item moves it and leaves an item anchor correct; an edit *inside* it
    /// could have moved or removed what the relative range names, so the operation is refused
    /// rather than re-targeted. The remedy is to re-anchor, which is the author's to do.
    #[error(
        "the item `{item}` in {file} changed since the plan was written — its fingerprint no longer \
         matches; re-anchor it with `restructure anchors`"
    )]
    ItemChanged { item: String, file: String },
    /// A continued run was handed a plan whose pending anchors are not the ones the run recorded.
    ///
    /// After each operation the run records a digest of the plan's pending anchors, then writes the
    /// plan back. A resume reads the plan and compares: a mismatch means the run stopped between
    /// the two, or somebody edited an anchor since — and either way the anchors no longer say where
    /// the tree's text is, so applying them would edit the wrong text. Refused, never repaired.
    #[error(
        "the plan's pending anchors do not match what the run recorded after operation {op}: it \
         stopped before the plan was written back, or an anchor was edited since. Resuming would \
         read positions that do not match the tree — run the remainder from a new plan file whose \
         anchors were written against the tree as it stands"
    )]
    PlanOutOfSync { op: usize },
    /// A continued run of a plan that anchors by item, over a journal that cannot vouch for it.
    ///
    /// The journal was written before plans were written back, so nothing says whether the plan's
    /// anchors describe the tree before or after the operations it completed.
    #[error(
        "this run continues a journal that already applied {applied} operation(s) before plans \
         were kept current, so the plan's item anchors cannot be checked against the tree — run \
         the remainder from a new plan file, or start the plan afresh"
    )]
    PlanUnverifiable { applied: usize },
    /// A loaded plan's file changed on disk since the store read it, so writing the store's copy
    /// back would discard what somebody wrote.
    #[error(
        "{plan} changed on disk since it was loaded — not overwriting it; unload it and load it \
         again"
    )]
    PlanChangedOnDisk { plan: String },
    /// The next operation of a held plan is stale, so the run is refused before any write.
    #[error("operation `{op}` of {plan} is stale ({reason}) — re-anchor it before applying")]
    StaleOperation {
        plan: String,
        op: String,
        reason: String,
    },
    /// A command only the index daemon's plan store can answer, asked of a run without one.
    #[error(
        "`restructure {command}` needs the index daemon — start one with ./run-index-daemon and \
         export TDDY_INDEX_SOCKET"
    )]
    NeedsIndexDaemon { command: String },
    /// `restructure warm` asked of a run with no daemon: a crate graph loaded by this process is
    /// dropped with it, so there is nothing for a warm to leave behind.
    #[error(
        "`restructure warm` loads a crate graph for an index daemon to hold, and there is no \
         daemon to hold it — start one with ./run-index-daemon and export TDDY_INDEX_SOCKET"
    )]
    WarmNeedsIndexDaemon,
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
        "rust-analyzer had not finished indexing after {seconds}s while {stage} (last progress: \
         {last}) and the wait was cancelled. Toolchain it resolved with: {environment}"
    )]
    IndexingIncomplete {
        seconds: u64,
        last: String,
        /// What rust-analyzer was actually launched against. A stall at `discovering sysroot`
        /// looks identical whether the toolchain was pinned, whether cargo/rustc resolved to
        /// real binaries or to rustup proxies, and whether it was reaching the network — this
        /// is the line that separates them in a CI log.
        environment: String,
        /// What the wait was waiting for when its caller stopped it — the sentence a heartbeat
        /// names it by (`warming the crate index`, `type inference at src/lib.rs:1`).
        ///
        /// A field rather than prose folded into `last`: `last` is the server's words, and a front
        /// end may want to render what the run was doing differently from what the server said.
        stage: String,
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
    /// The tree did not compile before a fresh `apply` wrote anything.
    ///
    /// Refused up front rather than applied, because [`RestructureError::AppliedTreeDoesNotCompile`]
    /// could not then be told from damage that was already there. None of the other classes is
    /// true of it: the plan is not malformed, no seam was refused, and no server was asked.
    #[error(
        "the tree does not compile before the plan runs: `{checked}` fails, so a failure after it \
         could not be told from one the plan caused. Nothing was written. Make the tree compile, \
         then apply again.\n{errors}"
    )]
    BaselineDoesNotCompile { checked: String, errors: String },
    /// An `apply` wrote its operations and the tree it left does not compile.
    ///
    /// Every operation was accepted — an assist's output the engine could not see through, a file a
    /// move left behind — and the compiler still rejects the result. The run is a failure, never
    /// "applied N of N": that line over a broken crate is the implicit failure this replaces. The
    /// edits stay on disk and in the journal so they can be inspected; the message says how to
    /// roll them back, because nothing here does it.
    #[error(
        "{applied} of {total} operation(s) were applied, and the tree no longer compiles: \
         `{checked}` fails. The edits are left on disk for inspection and nothing is committed. To \
         roll back, restore what the run touched from git ({touched} — `git checkout HEAD -- \
         <path>` for what HEAD holds, delete what it created, `git reset` what it staged) and \
         remove its journal, {journal}, so the plan can run again.\n{errors}"
    )]
    AppliedTreeDoesNotCompile {
        applied: usize,
        total: usize,
        checked: String,
        touched: String,
        journal: String,
        errors: String,
    },
    /// A transactional group's members were applied and the tree they left does not compile, so
    /// every file the group touched was restored from its journalled pre-images.
    ///
    /// Unlike [`RestructureError::AppliedTreeDoesNotCompile`], nothing of the group is left on disk:
    /// its members were declared to stand or fall together. Every operation before the group stays
    /// applied, and the run stops here.
    #[error("group `{group}` does not compile at its end, so it was rolled back: {errors}")]
    GroupDoesNotCompile { group: String, errors: String },
    /// `rustfmt` could not format a file the run wrote, or the edition to format it with could not
    /// be determined. The run is a failure rather than a skip: a file left unformatted is a CI
    /// failure, and one that rustfmt cannot parse is source the engine should not have written.
    #[error("the run's output could not be formatted: {file}: {errors}")]
    FormatFailed { file: String, errors: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RestructureError>;
