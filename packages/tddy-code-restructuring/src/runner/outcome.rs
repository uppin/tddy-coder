//! What a run produced, as values a caller acts on rather than lines on a stream.
//!
//! Pure data with no behaviour of its own, and imported by three crates — which is why it sits
//! apart from the entry points that build it.

// In scope for the doc links below, which name the routing table and the subcommand these shapes
// are one-per.
#[cfg(doc)]
use super::{dispatch, Command};

/// One thing wrong with an operation, as a value a caller can act on.
///
/// Returned rather than printed, because this library is not only a CLI: a server that speaks a
/// protocol on stdout has its stream corrupted by a library writing findings into it. The same
/// reasoning `backends::rust` gives for keeping progress a sink applies to results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The operation's index in the plan.
    pub operation: usize,
    pub detail: String,
}

/// How far a plan's journal got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanProgress {
    pub completed: usize,
    pub in_flight: usize,
    pub pending: usize,
    pub failed: usize,
}

/// What a `snapshot` did to a plan's header.
///
/// `rewritten` is the answer a reader acts on, and it is stated rather than inferred from the
/// count: a plan already snapshotting the working tree and one whose every hash moved both name
/// the same files, and only this tells them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRewrite {
    /// The plan whose header was read, as it was named on the command line.
    pub plan: String,
    /// How many paths that header names.
    pub paths: usize,
    /// Whether the header on disk differed from the working tree and was replaced.
    pub rewritten: bool,
}

/// What a whole run amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunSummary {
    pub applied: usize,
    pub total: usize,
    /// True when the run stopped because `stop_after` was reached rather than because it failed.
    pub stopped_early: bool,
}

/// What a dispatched run produced.
///
/// One variant per [`Command`], because the six entry points answer six different questions and
/// a single shape covering all of them would be mostly empty whichever one ran. It exists so that
/// [`dispatch`] can stay a routing table without becoming the place results are printed: a front
/// end renders the variant it gets, and a front end that speaks a protocol encodes it instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(RunSummary),
    Status(PlanProgress),
    Checked(Vec<Finding>),
    /// The range anchor a plan would carry, and the file it is a range in.
    Anchored {
        file: String,
        range: crate::edit::Range,
    },
    Verified(crate::verify::Comparison),
    Snapshotted(SnapshotRewrite),
}
