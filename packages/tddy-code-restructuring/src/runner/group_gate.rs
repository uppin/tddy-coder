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
use crate::spawn_record::SpawnRecorder;
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
    /// The plan index of the group's last member — where its end gate runs.
    last: usize,
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
        let last = *members
            .last()
            .expect("a group holds at least the operation that opens it");
        journal.append(
            &paths.journal,
            JournalRecord::group_started(index, group.clone(), members.clone()),
        )?;
        Ok(Some(GroupRun {
            group,
            last,
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
        self.last == index
    }

    /// Judge the group at its end. Passing journals `group_completed` and returns every member's
    /// resolution, in plan order, for the caller to refresh the plan store with; failing rolls the
    /// group back and returns the failure.
    ///
    /// # Errors
    ///
    /// [`RestructureError::GroupDoesNotCompile`] once the group has been rolled back; any other
    /// error the check raised (a manifest that cannot be read, say) likewise after the rollback,
    /// unchanged; [`RestructureError::CallerStopped`] when `cancel` fired during the check — the
    /// group stays open in the journal, which a resume rolls back before it runs anything.
    pub fn finish(
        self,
        root: &Path,
        paths: &StatePaths,
        journal: &mut Journal,
        cancel: &CancellationToken,
        spawns: &SpawnRecorder,
    ) -> Result<Vec<(usize, Resolution)>> {
        match gate_group(root, &self.group, journal, cancel, spawns) {
            Ok(()) => {
                journal.append(
                    &paths.journal,
                    JournalRecord::group_completed(self.last, self.group),
                )?;
                Ok(self.applied)
            }
            Err(RestructureError::CallerStopped) => Err(RestructureError::CallerStopped),
            Err(failure) => {
                roll_back_group(root, &self.group, paths, journal)?;
                Err(failure)
            }
        }
    }

    /// Pass `outcome` through, rolling back the group `slot` holds first when it is a failure.
    ///
    /// A group stands or falls whole, so a member that cannot be resolved or committed leaves the
    /// members before it undone, not applied under a group the journal still shows open. The
    /// failure returned is `outcome`'s own. A rollback that itself fails is returned instead: the
    /// tree is then in a state only the group's pre-images can account for, and the group stays
    /// open in the journal for a resume to roll back.
    ///
    /// [`RestructureError::CallerStopped`] is the exception, as at the group's end gate: nobody is
    /// waiting, and the open group is what a resume rolls back.
    pub fn roll_back_on_failure<T>(
        slot: &mut Option<GroupRun>,
        outcome: Result<T>,
        gate: &GroupGate<'_>,
        journal: &mut Journal,
    ) -> Result<T> {
        match outcome {
            Err(failure) if !matches!(failure, RestructureError::CallerStopped) => {
                if let Some(open) = slot.take() {
                    roll_back_group(gate.root, open.name(), gate.paths, journal)?;
                }
                Err(failure)
            }
            outcome => outcome,
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
        spawns: &SpawnRecorder,
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
        open.finish(gate.root, gate.paths, journal, gate.cancel, spawns)
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
    spawns: &SpawnRecorder,
) -> Result<()> {
    let touched: BTreeSet<String> = journal
        .group_records(group)
        .iter()
        .filter(|record| record.status == OpStatus::Completed)
        .filter_map(|record| record.edit.as_ref())
        .flat_map(touched_paths)
        .collect();
    let packages = owning_packages(root, touched.into_iter())?;
    match failing_check(root, &packages, spawns, cancel)? {
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
    // `group_started` is journalled at the group's first member.
    let first_member = started.op;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Position, Range, TextEdit, WorkspaceEdit};

    const THE_MEMBER: &str = concat!(
        "{\"v\":1,\"snapshot\":{}}\n",
        r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"pkg/src/lib.rs","path":"a"},"name":"b","group":"shapes"}"#,
        "\n"
    );

    /// A run that is inside the group `shapes`, one member in, with `pkg/src/lib.rs` already rewritten.
    struct AnOpenGroup {
        root: tempfile::TempDir,
        paths: StatePaths,
        journal: Journal,
        slot: Option<GroupRun>,
    }

    impl AnOpenGroup {
        /// `pkg/src/lib.rs` held "before" when the member began and holds "after" now; `manifest` is
        /// the text of the `pkg/Cargo.toml` owning it.
        fn with_manifest(manifest: &[u8]) -> Self {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("pkg/src")).unwrap();
            std::fs::write(root.path().join("pkg/Cargo.toml"), manifest).unwrap();
            std::fs::write(root.path().join("pkg/src/lib.rs"), "before\n").unwrap();
            let plan = Plan::parse(THE_MEMBER).unwrap();
            let paths = StatePaths::under(root.path());
            let mut journal = Journal::default();
            let mut slot = GroupRun::begin(&plan, 0, &paths, &mut journal).unwrap();
            let edit = WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: "pkg/src/lib.rs".to_string(),
                    edits: vec![TextEdit {
                        range: Range {
                            start: Position { line: 1, col: 1 },
                            end: Position { line: 1, col: 7 },
                        },
                        new_text: "after".to_string(),
                    }],
                }],
            };
            let resolved = Resolution::of(edit.clone());
            slot.as_mut()
                .unwrap()
                .pre_image(0, None, &resolved, root.path(), &paths, &mut journal)
                .unwrap();
            std::fs::write(root.path().join("pkg/src/lib.rs"), "after\n").unwrap();
            journal
                .append(
                    &paths.journal,
                    JournalRecord::completed(
                        0,
                        None,
                        edit,
                        Default::default(),
                        Default::default(),
                        Vec::new(),
                        Vec::new(),
                    ),
                )
                .unwrap();
            slot.as_mut().unwrap().applied(0, resolved);
            Self {
                root,
                paths,
                journal,
                slot,
            }
        }
    }

    fn the_library(root: &tempfile::TempDir) -> String {
        std::fs::read_to_string(root.path().join("pkg/src/lib.rs")).unwrap()
    }

    fn last_status(journal: &Journal) -> Option<OpStatus> {
        journal.records.last().map(|record| record.status)
    }

    #[test]
    fn a_failure_inside_an_open_group_rolls_it_back_and_is_returned_as_it_was() {
        // Given a group one member in
        let AnOpenGroup {
            root,
            paths,
            mut journal,
            mut slot,
        } = AnOpenGroup::with_manifest(b"[package]\nname = \"pkg\"\n");
        let cancel = CancellationToken::new();
        let gate = GroupGate {
            root: root.path(),
            paths: &paths,
            cancel: &cancel,
        };

        // When the next member fails to resolve
        let outcome = GroupRun::roll_back_on_failure(
            &mut slot,
            Err::<(), _>(RestructureError::MalformedPlan("no such symbol".into())),
            &gate,
            &mut journal,
        );

        // Then the failure is the member's own, the file is as it was and the group is closed
        assert_eq!(
            (
                outcome.map_err(|error| error.to_string()),
                the_library(&root),
                slot.is_none(),
                last_status(&journal),
            ),
            (
                Err("plan is malformed: no such symbol".to_string()),
                "before\n".to_string(),
                true,
                Some(OpStatus::GroupRolledBack),
            )
        );
    }

    #[test]
    fn a_cancellation_leaves_the_open_group_for_a_resume_to_roll_back() {
        // Given a group one member in
        let AnOpenGroup {
            root,
            paths,
            mut journal,
            mut slot,
        } = AnOpenGroup::with_manifest(b"[package]\nname = \"pkg\"\n");
        let cancel = CancellationToken::new();
        let gate = GroupGate {
            root: root.path(),
            paths: &paths,
            cancel: &cancel,
        };

        // When the caller stops waiting
        let outcome = GroupRun::roll_back_on_failure(
            &mut slot,
            Err::<(), _>(RestructureError::CallerStopped),
            &gate,
            &mut journal,
        );

        // Then the group is still open, in the run and in the journal, and the file is as the
        // member left it
        assert_eq!(
            (
                outcome.map_err(|error| error.to_string()),
                the_library(&root),
                slot.is_some(),
                journal.open_group().map(|open| open.group),
            ),
            (
                Err("the caller stopped waiting, so the request was abandoned".to_string()),
                "after\n".to_string(),
                true,
                Some("shapes".to_string()),
            )
        );
    }

    #[test]
    fn a_check_that_cannot_run_rolls_the_group_back_and_returns_its_own_error() {
        // Given a group whose package manifest cannot be read as text, so the end gate cannot say
        // which packages to check
        let AnOpenGroup {
            root,
            paths,
            mut journal,
            slot,
        } = AnOpenGroup::with_manifest(&[0xff, 0xfe]);
        let open = slot.expect("the member opened the group");

        // When the group is judged at its end
        let outcome = open.finish(
            root.path(),
            &paths,
            &mut journal,
            &CancellationToken::new(),
            &SpawnRecorder::discard(),
        );

        // Then the failure is the unreadable manifest's, not "does not compile", and the group is
        // undone
        assert_eq!(
            (
                matches!(outcome, Err(RestructureError::Io(_))),
                the_library(&root),
                last_status(&journal),
            ),
            (
                true,
                "before\n".to_string(),
                Some(OpStatus::GroupRolledBack)
            )
        );
    }
}
