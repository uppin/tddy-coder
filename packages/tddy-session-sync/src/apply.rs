//! What a delta is, and what applying one can conclude.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md` § Client — the managed mirror.

/// One tick's patch, reassembled from the frames of `StreamAgentActivityDelta`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Delta {
    /// The poll tick this patch belongs to. Several calls in one window share it, which is why the
    /// mirror de-duplicates by `seq` rather than by `call_id`.
    pub seq: u64,
    /// The tick this one follows. A gap against the mirror's `last_seq` is a lost broadcast.
    pub prev_seq: u64,
    /// The commit the patch applies onto.
    pub base_commit: String,
    /// `git diff --binary` output, limited to `scoped_paths`.
    pub patch: Vec<u8>,
    /// The paths this patch covers — the call's own files, not its whole poll window's.
    pub scoped_paths: Vec<String>,
}

/// What happened when a delta was offered to the mirror.
///
/// Three outcomes rather than a `bool` because the caller's next move differs in each: apply the
/// next delta, ignore a duplicate, or reconcile. Collapsing "already applied" into "applied" would
/// let a re-broadcast advance the mirror's sequence past a tick it never saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// The patch applied cleanly and the mirror advanced to `seq`.
    Applied,
    /// The mirror is already at or past this `seq`. Nothing was written.
    AlreadyApplied,
    /// The mirror cannot advance from where it is. Nothing was written.
    NeedsReconcile(ReconcileReason),
}

/// Why a mirror must resync from git rather than apply the delta it was offered.
///
/// Each variant carries what it saw, because these are what gets logged at `error` — a reconcile
/// reported as "diverged" with no values is a reconcile nobody can debug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileReason {
    /// The delta does not follow the last one applied.
    SequenceGap { expected: u64, found: u64 },
    /// The delta was cut from a different commit than the mirror is on.
    BaseCommitMismatch { expected: String, found: String },
    /// `git apply` refused the patch. Carries git's own message.
    PatchRejected { detail: String },
}

impl std::fmt::Display for ReconcileReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconcileReason::SequenceGap { expected, found } => write!(
                f,
                "activity sequence gap: expected seq {expected}, received {found}"
            ),
            ReconcileReason::BaseCommitMismatch { expected, found } => write!(
                f,
                "delta was cut from {found} but the mirror is on {expected}"
            ),
            ReconcileReason::PatchRejected { detail } => {
                write!(f, "git apply refused the patch: {detail}")
            }
        }
    }
}
