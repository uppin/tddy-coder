//! The gate at a transactional group's end, and the rollback when the group does not pass it.
//!
//! Some refactors cannot compile step by step — change a type, then adapt every use — so a plan
//! marks consecutive operations as one group. Ungrouped operations keep the end-of-run gate
//! ([`super::refuse_a_broken_result`]) and its leave-on-disk contract; a group is judged at its own
//! end instead, by `cargo check --all-targets` over the packages its members touched, and a group
//! that fails there is rolled back exactly from the pre-images its members journalled
//! ([`crate::journal::PreImage`]): contents rewritten, created files removed, renames undone.
//! Everything before the group stays applied.
//!
//! Both apply loops call these — the command line's ([`super::apply_from_store`]) and the index
//! daemon's — and a resume that finds a group open ([`Journal::open_group`]) rolls it back before
//! it runs anything.

use std::collections::BTreeSet;
use std::path::Path;

use tokio_util::sync::CancellationToken;

use crate::apply::touched_paths;
use crate::journal::{Journal, JournalRecord, OpStatus, PreImage};
use crate::{LedgerCheckpoint, OpId, Plan, Resolution, RestructureError, Result};

use super::compile_gate::{failing_check, owning_packages};
use super::StatePaths;

/// A group being applied by a run: what an apply loop carries from its first member to its last.
///
/// The loop calls [`GroupRun::begin`] at each operation while it holds none, [`GroupRun::pre_image`]
/// before committing a member, [`GroupRun::applied`] after, and [`GroupRun::finish`] once
/// [`GroupRun::closes_at`] says the member was the last — which gates the group, and either
/// completes it or rolls it back. Members are **not** refreshed into the plan store while the group
/// is open: [`GroupRun::finish`] hands them back so the caller does that at the group's end, once
/// the tree is known to compile.
pub struct GroupRun {
    group: String,
    members: Vec<usize>,
    captured: BTreeSet<String>,
    applied: Vec<(usize, Resolution)>,
}

impl GroupRun {
    /// Begin the group the operation at `index` belongs to, journalling `group_started` naming the
    /// members from `index` on. `None` for an ungrouped operation.
    pub fn begin(
        plan: &Plan,
        index: usize,
        paths: &StatePaths,
        journal: &mut Journal,
    ) -> Result<Option<GroupRun>> {
        let Some(group) = plan.ops.get(index).and_then(|op| op.group.clone()) else {
            return Ok(None);
        };
        let members: Vec<usize> = plan
            .ops
            .iter()
            .enumerate()
            .skip(index)
            .take_while(|(_, op)| op.group.as_deref() == Some(group.as_str()))
            .map(|(member, _)| member)
            .collect();
        journal.append(
            &paths.journal,
            JournalRecord::group_started(index, group.clone(), members.clone()),
        )?;
        Ok(Some(GroupRun {
            group,
            members,
            captured: BTreeSet::new(),
            applied: Vec::new(),
        }))
    }

    /// The group's id.
    pub fn name(&self) -> &str {
        &self.group
    }

    /// Journal the bytes of every file member `index`'s edit touches that no earlier member of the
    /// group has — before the member's `in_flight` record, so a crash anywhere inside the group
    /// leaves what a rollback restores from.
    pub fn pre_image(
        &mut self,
        index: usize,
        id: Option<&OpId>,
        resolved: &Resolution,
        root: &Path,
        paths: &StatePaths,
        journal: &mut Journal,
    ) -> Result<()> {
        let mut images = Vec::new();
        for path in touched_paths(&resolved.edit) {
            if self.captured.insert(path.clone()) {
                images.push(PreImage::capture(root, &path)?);
            }
        }
        if images.is_empty() {
            return Ok(());
        }
        journal.append(
            &paths.journal,
            JournalRecord::pre_imaged(index, id.cloned(), self.group.clone(), images),
        )
    }

    /// Keep member `index`'s resolution, for the plan refresh at the group's end.
    pub fn applied(&mut self, index: usize, resolved: Resolution) {
        self.applied.push((index, resolved));
    }

    /// Whether member `index` is the last of the group.
    pub fn closes_at(&self, index: usize) -> bool {
        self.members.last() == Some(&index)
    }

    /// Judge the group at its end. Passing journals `group_completed` and returns every member's
    /// resolution, in plan order, for the caller to refresh the plan store with; failing rolls the
    /// group back and returns the refusal.
    ///
    /// # Errors
    ///
    /// [`RestructureError::GroupDoesNotCompile`] once the group has been rolled back;
    /// [`RestructureError::CallerStopped`] when `cancel` fired during the check — the group stays
    /// open in the journal, which a resume rolls back before it runs anything.
    pub fn finish(
        self,
        root: &Path,
        paths: &StatePaths,
        journal: &mut Journal,
        cancel: &CancellationToken,
    ) -> Result<Vec<(usize, Resolution)>> {
        match gate_group(root, &self.group, journal, cancel) {
            Ok(()) => {
                let last = self.members.last().copied().unwrap_or_default();
                journal.append(
                    &paths.journal,
                    JournalRecord::group_completed(last, self.group),
                )?;
                Ok(self.applied)
            }
            Err(refusal @ RestructureError::GroupDoesNotCompile { .. }) => {
                roll_back_group(root, &self.group, paths, journal)?;
                Err(refusal)
            }
            Err(other) => Err(other),
        }
    }
}

/// What a run needs beside the journal to enter, gate and roll back a group.
pub struct GroupGate<'a> {
    pub root: &'a Path,
    pub paths: &'a StatePaths,
    pub cancel: &'a CancellationToken,
}

/// What committing one operation left for the plan store to refresh.
pub enum Settled {
    /// The operation belongs to a group that is still open: nothing is refreshed yet.
    Pending,
    /// Operations whose edits are final, in plan order: the operation itself when it is ungrouped,
    /// every member once its group has compiled.
    Ready(Vec<(usize, Resolution)>),
}

impl GroupRun {
    /// Before committing operation `index`: begin its group when it opens one and `slot` holds none,
    /// then journal the pre-images the group's open member needs. Does nothing for an ungrouped
    /// operation.
    pub fn enter(
        slot: &mut Option<GroupRun>,
        plan: &Plan,
        index: usize,
        id: Option<&OpId>,
        resolved: &Resolution,
        gate: &GroupGate<'_>,
        journal: &mut Journal,
    ) -> Result<()> {
        if slot.is_none() {
            *slot = GroupRun::begin(plan, index, gate.paths, journal)?;
        }
        match slot.as_mut() {
            Some(open) => open.pre_image(index, id, resolved, gate.root, gate.paths, journal),
            None => Ok(()),
        }
    }

    /// After committing operation `index`: keep it for its group, and judge the group when it was
    /// the last member (`on_check` is told the group's name first). An ungrouped operation is
    /// settled at once.
    ///
    /// # Errors
    ///
    /// Those of [`GroupRun::finish`].
    pub fn settle(
        slot: &mut Option<GroupRun>,
        index: usize,
        resolved: Resolution,
        gate: &GroupGate<'_>,
        journal: &mut Journal,
        on_check: impl FnOnce(&str),
    ) -> Result<Settled> {
        let Some(mut open) = slot.take() else {
            return Ok(Settled::Ready(vec![(index, resolved)]));
        };
        open.applied(index, resolved);
        if !open.closes_at(index) {
            *slot = Some(open);
            return Ok(Settled::Pending);
        }
        on_check(open.name());
        open.finish(gate.root, gate.paths, journal, gate.cancel)
            .map(Settled::Ready)
    }
}

/// Refuse the group `group` when the tree its members left does not compile.
///
/// Checked over the packages owning every file the group's members journalled an edit to.
///
/// # Errors
///
/// [`crate::RestructureError::GroupDoesNotCompile`] naming the group and the compiler's errors. The
/// caller rolls the group back ([`roll_back_group`]) before returning it.
pub fn gate_group(
    root: &Path,
    group: &str,
    journal: &Journal,
    cancel: &CancellationToken,
) -> Result<()> {
    let touched: BTreeSet<String> = journal
        .group_records(group)
        .iter()
        .filter(|record| record.status == OpStatus::Completed)
        .filter_map(|record| record.edit.as_ref())
        .flat_map(touched_paths)
        .collect();
    let packages = owning_packages(root, touched.into_iter())?;
    match failing_check(root, &packages, cancel)? {
        None => Ok(()),
        Some((_, errors)) => Err(RestructureError::GroupDoesNotCompile {
            group: group.to_string(),
            errors,
        }),
    }
}

/// Restore every file the group `group` touched from the pre-images its members journalled, then
/// journal `group_rolled_back`.
///
/// Pre-images are restored in reverse of the order they were written, so a file a later member
/// touched first is restored from what the group first saw.
///
/// The ledger checkpoint is put back to the last operation before the group *before* the rollback
/// is journalled: a checkpoint behind the journal is the lag a resume accepts, one ahead of it is a
/// divergence it refuses.
pub fn roll_back_group(
    root: &Path,
    group: &str,
    paths: &StatePaths,
    journal: &mut Journal,
) -> Result<()> {
    let records = journal.group_records(group);
    let (Some(started), Some(reached)) = (records.first(), records.last()) else {
        return Err(RestructureError::MalformedPlan(format!(
            "the journal never started the group `{group}`, so there is nothing to roll back"
        )));
    };
    let first_member = started.members.first().copied().unwrap_or(started.op);
    let reached = reached.op;

    for image in journal.group_pre_images(group).iter().rev() {
        image.restore(root)?;
    }

    let before_group = journal
        .completed()
        .filter(|record| record.op < first_member)
        .max_by_key(|record| record.op)
        .map(|record| record.op);
    match before_group {
        Some(op) => LedgerCheckpoint {
            op,
            ledger: journal.fold_through(op),
        }
        .write(&paths.ledger)?,
        None => match std::fs::remove_file(&paths.ledger) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }

    journal.append(
        &paths.journal,
        JournalRecord::group_rolled_back(reached, group.to_string()),
    )
}

/// Roll back the group an interrupted run left open, if there is one, so a resume starts from the
/// tree as it was before the group.
///
/// What a run continuing a journal does before it reads a single anchor: the group's end gate must
/// judge members one run applied, so a partial group is undone and applied again whole.
pub fn roll_back_an_open_group(
    root: &Path,
    paths: &StatePaths,
    journal: &mut Journal,
    progress: &crate::backends::rust::ProgressSink,
) -> Result<()> {
    let Some(open) = journal.open_group() else {
        return Ok(());
    };
    progress(&format!(
        "group `{}` was interrupted before it finished; rolling it back",
        open.group
    ));
    roll_back_group(root, &open.group, paths, journal)
}
