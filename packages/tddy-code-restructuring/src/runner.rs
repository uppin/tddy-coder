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
mod compile_gate;
mod entry_points;
mod options;
mod outcome;
mod rehearsal;

pub use comparison::verify;
pub use compile_gate::{refuse_a_broken_baseline, refuse_a_broken_result, AppliedRun};
pub use entry_points::{
    anchors, apply, check, dispatch, item_anchors, registry_for, run, snapshot, status,
};
pub use options::{command_of, parse_options, Command, Options};
pub use outcome::{Finding, Outcome, PlanProgress, RunSummary, SnapshotRewrite};

use crate::apply::{apply_workspace_edit, ensure_git_worktree, hash_touched_files};
use crate::journal::{Journal, JournalRecord, ResumeDecision};
use crate::{LedgerCheckpoint, Plan, PositionLedger, RestructureError, Result};
use sha2::{Digest, Sha256};
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
/// The gate is as wide as the [`StatePaths`] it is given. With [`StatePaths::for_plan`] it is
/// plan-scoped: a *completed* plan's journal no longer refuses the next plan under the same root,
/// and a resume resumes the plan that was named. With [`StatePaths::under`] it stays repo-scoped —
/// one journal for every plan under one root — so a host driving the loop that way has to serialize
/// its runs per root itself.
///
/// A journal left at `<root>/.restructure/` by a run made before run state was keyed by the plan is
/// adopted or refused, never stepped over: see [`adopt_or_refuse_repo_scoped_state`].
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
    open_run_after(plan, root, paths, options, || Ok(()))
}

/// [`open_run`], with one more refusal — `before_writing` — run after its own and before it writes
/// anything.
///
/// For a gate that is expensive, which is why it comes last: the baseline compile check
/// ([`refuse_a_broken_baseline`]) takes minutes, so a plan the cheap refusals would turn away is
/// turned away first. And for a gate whose refusal says "Nothing was written", which is why it comes
/// before `.restructure/` is created — that directory is the first thing a run writes.
pub fn open_run_after(
    plan: &Plan,
    root: &Path,
    paths: &StatePaths,
    options: &Options,
    before_writing: impl FnOnce() -> Result<()>,
) -> Result<Journal> {
    ensure_git_worktree(root)?;

    let continuing = options.continues_a_journal();
    // A fresh run's refusals read and never write, so they all come before `.restructure/` exists.
    // A continuing run may adopt a repository-scoped journal, which moves it into the directory.
    if !continuing {
        refuse_repo_scoped_state(root, paths)?;
        if !Journal::load(&paths.journal)?.records.is_empty() {
            return Err(RestructureError::JournalExists);
        }
        plan.verify_snapshot(root)?;
    }
    before_writing()?;

    paths.ensure_self_ignoring()?;
    if continuing {
        adopt_or_refuse_repo_scoped_state(root, paths, continuing, &options.progress)?;
    }

    let journal = Journal::load(&paths.journal)?;
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

/// Where a run's write-ahead state lives, under `<root>/.restructure/`.
///
/// [`StatePaths::for_plan`] puts a run's journal and ledger in a directory of the plan's own, which
/// is what lets one plan follow another under a single root. [`StatePaths::under`] is the
/// repository-scoped layout that came before it, kept for a host driving the loop itself with no
/// plan path to key on — and it is why [`open_run`]'s refusal is repo-scoped for such a host.
///
/// The file names are **private**, so `.restructure/`'s layout is this crate's own business: a
/// host drives the apply loop through [`StatePaths::for_plan`], [`open_run`], [`restore_ledger`]
/// and [`commit_operation`] and never re-derives a path. That is what kept re-keying the journal to
/// a plan identity from being a breaking change for anything outside.
pub struct StatePaths {
    /// The append-only event journal — the record of what a run has done.
    journal: PathBuf,
    /// The position-ledger checkpoint — a projection over `journal`, rebuildable from it.
    ledger: PathBuf,
}

/// The directory under `.restructure/` that every run's state lives in.
const STATE_DIRECTORY: &str = ".restructure";
/// The append-only event journal's file name within a run's state directory.
const JOURNAL_FILE: &str = "journal.jsonl";
/// The position-ledger checkpoint's file name within a run's state directory.
const LEDGER_FILE: &str = "ledger.json";

/// Where a plan's run state lives, keyed by the plan rather than by the repository.
///
/// [`StatePaths::under`] puts the journal and ledger at `<root>/.restructure/`, one set per
/// repository — so a **completed** plan blocks the next one with *"a journal already exists for this
/// plan — pass `--resume`"*, and `--resume` would resume the wrong plan against the new plan's
/// coordinates. The backlog calls this the single largest tax on a multi-layer move, and every
/// `#carve` node is multi-layer by construction: one plan carves a flat module, the next moves it.
///
/// The directory is `<root>/.restructure/<plan stem>-<digest>`: the stem is there so a person
/// reading `.restructure/` can tell which run is which, and the digest is what actually keys it,
/// because two plans in different directories may share a stem. A **relative** plan path is read
/// against `root` and the result normalised lexically — no disk is touched, so a plan that has not
/// been written yet still has a state directory — which is what makes `restructure apply
/// tmp/plan.jsonl` and `restructure apply /repo/tmp/plan.jsonl` the same run to `--resume`.
///
/// # Errors
///
/// Refuses a path that names no plan file — `/` or one ending in `..` — because there is no stable
/// key in it, and a run whose state directory depended on the process's directory would resume
/// somebody else's journal.
pub fn state_directory_for_plan(root: &Path, plan: &Path) -> Result<PathBuf> {
    Ok(root.join(STATE_DIRECTORY).join(plan_key(root, plan)?))
}

/// The directory name a plan's state lives under: its own stem, and the digest that keys it.
fn plan_key(root: &Path, plan: &Path) -> Result<String> {
    let identity = plan_identity(root, plan);
    let stem = identity
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(readable)
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "`{}` names no plan file, so a run of it has no state directory to be keyed by — \
                 name the plan itself",
                plan.display()
            ))
        })?;

    let digest = format!(
        "{:x}",
        Sha256::digest(identity.to_string_lossy().as_bytes())
    );
    Ok(format!("{stem}-{}", &digest[..16]))
}

/// The plan path as this root sees it: absolute, and lexically normalised.
///
/// Lexical rather than canonical because a plan is keyed before it is read — `restructure snapshot`
/// writes one, and a `check` of a plan that does not exist has to refuse for *that* reason rather
/// than for a state directory it could not name.
fn plan_identity(root: &Path, plan: &Path) -> PathBuf {
    let absolute = if plan.is_absolute() {
        plan.to_path_buf()
    } else {
        root.join(plan)
    };

    let mut normalised = PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalised.pop();
            }
            other => normalised.push(other),
        }
    }
    normalised
}

/// `stem` with everything a directory name should not carry replaced by `_`.
fn readable(stem: &str) -> String {
    stem.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '_',
        })
        .take(40)
        .collect()
}

/// Adopt or refuse a journal a run made before run state was keyed by the plan.
///
/// A repository-scoped journal cannot be attributed to a plan: it is whatever ran last under this
/// root. Adopting one silently is the failure this change exists to prevent — `--resume` would
/// replay another plan's operations against these coordinates — so it is adopted only when the run
/// says it is continuing a journal and this plan has none of its own, and it says so when it does.
/// Otherwise it is refused, naming the file, because leaving it in place would make a later
/// `--resume` reach for it again.
fn adopt_or_refuse_repo_scoped_state(
    root: &Path,
    paths: &StatePaths,
    continuing: bool,
    progress: &crate::backends::rust::ProgressSink,
) -> Result<()> {
    let Some(legacy) = repo_scoped_journal(root, paths)? else {
        return Ok(());
    };

    if continuing && Journal::load(&paths.journal)?.records.is_empty() {
        std::fs::rename(&legacy, &paths.journal)?;
        let ledger = root.join(STATE_DIRECTORY).join(LEDGER_FILE);
        if ledger.exists() {
            std::fs::rename(&ledger, &paths.ledger)?;
        }
        progress(&format!(
            "adopted the repository-scoped journal {} as this plan's own, at {}",
            legacy.display(),
            paths.journal.display()
        ));
        return Ok(());
    }

    Err(RestructureError::RepoScopedJournal {
        path: legacy.display().to_string(),
    })
}

/// The repository-scoped journal standing above a plan's own state, when there is one to reckon
/// with.
///
/// Loaded rather than merely looked for: a journal whose records cannot be read is a refusal of its
/// own, and reporting it as state belonging to an unknown plan would hide what is actually wrong
/// with it.
fn repo_scoped_journal(root: &Path, paths: &StatePaths) -> Result<Option<PathBuf>> {
    let legacy = root.join(STATE_DIRECTORY).join(JOURNAL_FILE);
    if legacy == paths.journal || !legacy.exists() {
        return Ok(None);
    }
    if Journal::load(&legacy)?.records.is_empty() {
        return Ok(None);
    }
    Ok(Some(legacy))
}

/// Refuse a run that would read past a repository-scoped journal, for the entry points that only
/// read.
///
/// `status` reports a plan's progress out of that plan's journal, and a repository-scoped one
/// standing beside it belongs to nothing this call can name — so it is reported rather than stepped
/// over, which is the same decision [`adopt_or_refuse_repo_scoped_state`] makes for a run that
/// writes.
pub(super) fn refuse_repo_scoped_state(root: &Path, paths: &StatePaths) -> Result<()> {
    match repo_scoped_journal(root, paths)? {
        None => Ok(()),
        Some(legacy) => Err(RestructureError::RepoScopedJournal {
            path: legacy.display().to_string(),
        }),
    }
}

impl StatePaths {
    /// The state paths for a run against `root`. Derives paths only; touches no disk.
    ///
    /// Repository-scoped: see [`StatePaths::for_plan`] for the layout a run that knows its plan
    /// uses, and [`state_directory_for_plan`] for why keying by the root alone is a tax.
    pub fn under(root: &Path) -> Self {
        Self::in_directory(&root.join(STATE_DIRECTORY))
    }

    /// The state paths for a run of `plan` against `root`. Derives paths only; touches no disk.
    ///
    /// # Errors
    ///
    /// Refuses for the one reason [`state_directory_for_plan`] does: a path that names no plan file.
    pub fn for_plan(root: &Path, plan: &Path) -> Result<Self> {
        Ok(Self::in_directory(&state_directory_for_plan(root, plan)?))
    }

    fn in_directory(dir: &Path) -> Self {
        Self {
            journal: dir.join(JOURNAL_FILE),
            ledger: dir.join(LEDGER_FILE),
        }
    }

    fn ensure_self_ignoring(&self) -> Result<()> {
        let dir = self
            .journal
            .parent()
            .expect("state paths live in a directory");
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(".gitignore"), "*\n")?;
        // A plan's own directory sits inside `.restructure/`, and a `.gitignore` in it says
        // nothing about the directory holding it.
        if let Some(above) = dir
            .parent()
            .filter(|above| above.ends_with(STATE_DIRECTORY))
        {
            std::fs::write(above.join(".gitignore"), "*\n")?;
        }
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
