//! The entry points for restructuring subcommands, one per [`Command`].
//!
//! The plan is a command log and is never rewritten. Each operation's resolved edit is appended to
//! an event journal, and the position ledger is a projection over that journal — which is what
//! makes an interrupted run resumable.
//!
//! **Nothing here prints.** Every entry point returns its result — findings, progress, a summary,
//! an anchor, a comparison — and its live account goes to the sinks its caller installed in
//! [`Options`]. `backends::rust` states the reason for progress, and it is no less true of
//! results: anything that speaks a protocol on stdout, a persistent server most obviously, would
//! have its stream corrupted by this library writing into it. [`crate::restructure_cli`] is the
//! one front end that owns a console, so it is the one module that prints.

mod budget;
mod comparison;
mod entry_points;
mod options;
mod outcome;
mod rehearsal;

pub use comparison::verify;
pub use entry_points::{anchors, apply, check, dispatch, registry_for, run, snapshot, status};
pub use options::{command_of, parse_options, Command, Options};
pub use outcome::{Finding, Outcome, PlanProgress, RunSummary, SnapshotRewrite};

use crate::apply::{apply_workspace_edit, ensure_git_worktree, hash_touched_files};
use crate::journal::{Journal, JournalRecord, ResumeDecision};
use crate::{LedgerCheckpoint, Plan, PositionLedger, RestructureError, Result};
use std::path::{Path, PathBuf};

// The run state — the `.restructure/` write-ahead sequence — stays in this module rather than
// moving to a child of it, because `StatePaths`' file names are private: `.restructure/`'s layout
// is this crate's own business. The write-ahead calls below read those fields, and so does
// `entry_points::status`. A descendant module can read them; a sibling could only if they were
// widened, which is the one thing keeping them private is for.

/// Write one resolved operation to disk, journalling it either side of the write.
///
/// The write-ahead sequence a caller would otherwise have to reproduce: hash what the edit touches,
/// append an in-flight record, apply the edit, hash again, append the completed record carrying both
/// hashes, then checkpoint the ledger folded through this operation. A crash between any two of
/// those steps leaves a journal [`crate::journal::Journal::resume_decision`] can read, which is what
/// makes an interrupted run resumable — so a caller driving the loop itself must commit through here
/// rather than calling [`crate::apply::apply_workspace_edit`] directly.
///
/// The caller owns the ordering: `index` is the operation's position in the plan, and operations
/// must be committed in the order the plan states them, because `ledger` is a projection over the
/// journal and translating a later anchor depends on every earlier edit having been recorded.
pub fn commit_operation(
    index: usize,
    resolved: &crate::Resolution,
    root: &Path,
    paths: &StatePaths,
    journal: &mut Journal,
    ledger: &mut PositionLedger,
) -> Result<()> {
    let pre = hash_touched_files(root, &resolved.edit)?;
    journal.append(&paths.journal, JournalRecord::in_flight(index, pre.clone()))?;

    apply_workspace_edit(root, &resolved.edit)?;

    let post = hash_touched_files(root, &resolved.edit)?;
    journal.append(
        &paths.journal,
        JournalRecord::completed(
            index,
            resolved.edit.clone(),
            pre,
            post,
            resolved.report.clone(),
            resolved.notes.clone(),
        ),
    )?;

    ledger.record(&resolved.edit);
    LedgerCheckpoint {
        op: index,
        ledger: journal.fold_through(index),
    }
    .write(&paths.ledger)
}

/// Open the journal a run under `root` will append to, refusing a run that must not start.
///
/// **This is the only concurrency gate that exists.** There is no lock file: what stops a second run
/// from interleaving with a first is [`RestructureError::JournalExists`], raised when a journal is
/// already there and the run did not say it was continuing one (`--resume`, or `--from`). A caller
/// that passes either takes over the journal it finds, whatever wrote it.
///
/// And because [`StatePaths`] is keyed by `root` alone, that gate is repo-scoped rather than
/// plan-scoped: a *completed* plan's journal refuses the next plan under the same root, and a resume
/// would resume the journal on disk rather than the plan that was named. A host serving several
/// callers therefore has to serialize them per root itself — this call cannot do it, and a run that
/// starts anyway will write into another run's journal.
///
/// Also refused: a root that is not a git worktree (the edits are applied with git), and a journal
/// whose last record cannot be decided either way ([`RestructureError::IndeterminateJournal`]).
/// A fresh run — one not continuing a journal — verifies the plan's snapshot against `root` here,
/// so a resumed run does not re-verify a snapshot its own earlier operations have already changed.
pub fn open_run(
    plan: &Plan,
    root: &Path,
    paths: &StatePaths,
    options: &Options,
) -> Result<Journal> {
    ensure_git_worktree(root)?;
    paths.ensure_self_ignoring()?;

    let journal = Journal::load(&paths.journal)?;
    let continuing = options.resume || options.from.is_some();

    if !journal.records.is_empty() && !continuing {
        return Err(RestructureError::JournalExists);
    }
    if !continuing {
        plan.verify_snapshot(root)?;
    }
    if let Some(ResumeDecision::Abort(op)) = journal.resume_decision(root)? {
        return Err(RestructureError::IndeterminateJournal { op });
    }
    Ok(journal)
}

/// The position ledger a run continues from, folded out of the journal it has been given.
///
/// The ledger on disk is a checkpoint, not the record: the journal is. So this verifies the
/// checkpoint against the journal — raising [`RestructureError::CheckpointDivergence`] when the two
/// disagree, which means something other than a run of this library wrote under `root` — and then
/// returns the ledger the journal itself folds to. A caller must fold from the same journal
/// [`open_run`] returned, since a ledger folded from one journal cannot translate anchors against
/// another.
pub fn restore_ledger(journal: &Journal, paths: &StatePaths) -> Result<PositionLedger> {
    if let Some(checkpoint) = LedgerCheckpoint::load(&paths.ledger)? {
        journal.verify_checkpoint(&checkpoint)?;
    }
    Ok(journal.fold())
}

/// Where a run's write-ahead state lives: `<root>/.restructure/`.
///
/// Keyed by `root` and nothing else — no plan identity is in the path — so every plan run under one
/// root shares one journal and one ledger. That is what makes [`open_run`]'s refusal repo-scoped,
/// and it is why a host must not run two plans against the same root at once.
/// The file names are **private**, so `.restructure/`'s layout is this crate's own business: a
/// host drives the apply loop through [`StatePaths::under`], [`open_run`], [`restore_ledger`] and
/// [`commit_operation`] and never re-derives a path. That is what keeps the change re-keying the
/// journal to a plan identity — recorded in
/// `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` — from being
/// a breaking change for anything outside.
pub struct StatePaths {
    /// The append-only event journal — the record of what a run has done.
    journal: PathBuf,
    /// The position-ledger checkpoint — a projection over `journal`, rebuildable from it.
    ledger: PathBuf,
}

/// Where a plan's run state lives, keyed by the plan rather than by the repository.
///
/// `StatePaths::under` puts the journal and ledger at `<root>/.restructure/`, one set per
/// repository — so a **completed** plan blocks the next one with *"a journal already exists for this
/// plan — pass `--resume`"*, and `--resume` would resume the wrong plan against the new plan's
/// coordinates. The backlog calls this the single largest tax on a multi-layer move, and every
/// `#carve` node is multi-layer by construction: one plan carves a flat module, the next moves it.
///
/// # Errors
///
/// Refuses when the plan path cannot be made into a stable key.
pub fn state_directory_for_plan(_root: &Path, _plan: &Path) -> Result<PathBuf> {
    // TODO(restructure-clusters): implement
    todo!("state_directory_for_plan: key run state by the plan, not the repository")
}

impl StatePaths {
    /// The state paths for a run against `root`. Derives paths only; touches no disk.
    pub fn under(root: &Path) -> Self {
        let dir = root.join(".restructure");
        Self {
            journal: dir.join("journal.jsonl"),
            ledger: dir.join("ledger.json"),
        }
    }

    fn ensure_self_ignoring(&self) -> Result<()> {
        let dir = self
            .journal
            .parent()
            .expect("state paths live in a directory");
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(".gitignore"), "*\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout pinned from inside the module that owns it, which is where it belongs now that
    /// the fields are private: `.restructure/`'s file names are this crate's own business, and the
    /// change that re-keys the journal to a plan identity must not be a breaking change for a host.
    #[test]
    fn derives_a_runs_journal_and_ledger_under_the_root_it_is_given() {
        // Given a workspace root
        let root = Path::new("/trees/one");

        // When the state paths for a run against it are derived
        let paths = StatePaths::under(root);

        // Then both live in that root's own `.restructure/`, and nowhere near this process's
        // directory
        assert_eq!(
            (paths.journal, paths.ledger),
            (
                PathBuf::from("/trees/one/.restructure/journal.jsonl"),
                PathBuf::from("/trees/one/.restructure/ledger.json")
            )
        );
    }
}
