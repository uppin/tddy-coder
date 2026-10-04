use super::super::refuse_a_broken_result;
use crate::backends::rust::ProgressSink;
use crate::Result;

use super::super::commit_operation;

use super::progress_line;

use super::report_visibility;

use crate::{
    plan_store::PlanStore,
    registry::Workspace,
    runner::{
        entry_points::anchor_entry_points,
        group_gate::{GroupGate, GroupRun, Settled},
        restore_ledger, resume, AppliedRun, StatePaths,
    },
    BackendRegistry, Journal, Resolution,
};

use crate::Overlay;

use super::registry_for;

use super::super::refuse_a_broken_baseline;

use crate::PositionLedger;

use crate::Plan;

use crate::RestructureError;

use super::super::RunSummary;

use super::super::Finding;

use tokio_util::sync::CancellationToken;

use tddy_lsp::client::LspClient;

use std::sync::Arc;

use super::super::Options;

use crate::plan_store::PlanKey;

use std::path::Path;

/// Apply the plan `key` names from `store`, starting at `from` (or where its journal left off),
/// refreshing the plan's pending operations after each one and flushing it at the end.
///
/// What every front end runs: the daemon over its long-lived store, a one-shot `apply` over a store
/// that lives for the run. The plan is the store's copy, not the file: `options.target` names
/// nothing here, and a file edited since it was loaded changes nothing about what runs.
///
/// **The store's anchors are current.** After each committed operation the plan's pending operations
/// are rewritten for the tree it left and the plan is written back, so what the file says is what
/// the tree holds as of the last completed operation. A run therefore translates anchors through the
/// edits *of this run* only — a ledger folded from the journal would carry the edits of earlier runs
/// a second time — and a resume reads anchors that match the tree it resumes on. The write follows
/// the journal's record of the operation, so a crash between the two leaves the file one operation
/// behind the journal.
///
/// A dry run writes nothing: no journal record, no refresh, no flush. Neither does a run that is
/// refused before its first operation: the plan file is as it was.
///
/// # Errors
///
/// [`RestructureError::PlanChangedOnDisk`] when the plan's file changed since it was loaded, at the
/// first write. The operations committed before it stay on disk and in the journal.
pub fn apply_from_store(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("apply requires a rust-analyzer LSP session".into())
    })?;
    refuse_a_stale_pending_op(store, key, &options)?;
    let summary = apply_held_plan(root, store, key, &options, client, &cancel)?;
    // A run that refused or failed writes nothing here: the operations it did commit wrote the plan
    // as they landed ([`record_applied_op`]), and one that never started must leave the plan file
    // as it found it — "nothing was written" is what its refusal says.
    if !options.dry_run {
        store.flush(key)?;
    }
    Ok(summary)
}

/// A run that has been opened: its journal, the plan it executes, and where in it to start.
///
/// What [`open_plan_run`] hands back, so the loop that drives it — the command line's, which
/// reports lines, and the daemon's, which reports events — starts from the same state.
pub struct PlanRun {
    pub journal: Journal,
    /// The plan to execute, every item anchor lowered into the range it names on the tree the run
    /// starts on.
    pub plan: Plan,
    pub paths: StatePaths,
    /// Translates `plan`'s anchors: through the edits **this run** commits and nothing earlier, since
    /// the plan was written back to match the tree — unless the journal `legacy`, in which case it
    /// starts from the journal's own fold. See [`apply_from_store`].
    pub ledger: PositionLedger,
    /// The journal was written before plans were kept current, so this run continues it as such: its
    /// operations are journalled without ids and the plan is neither refreshed nor written back —
    /// mixing the two epochs in one plan would leave anchors that match neither.
    pub legacy: bool,
    /// The index of the first operation to execute.
    pub start: usize,
}

/// Open the run of `plan`, a plan a store holds at `plan_path`: the journal, with its refusals, and
/// item anchors lowered, in the one order every apply loop uses ([`open_run_resolving_anchors`]).
///
/// Takes the plan rather than the store, so a host whose store is shared can copy the plan out and
/// not hold the store through a baseline compile check that takes minutes.
pub fn open_plan_run(
    plan: &Plan,
    plan_path: &Path,
    root: &Path,
    options: &Options,
    registry: &mut BackendRegistry,
    cancel: &CancellationToken,
) -> Result<PlanRun> {
    let paths = StatePaths::for_plan(root, plan_path)?;
    // Item anchors resolve, and then the baseline compile check runs, both before `.restructure/`
    // exists — see `open_run_resolving_anchors` for why in that order.
    let (journal, lowered) = anchor_entry_points::open_run_resolving_anchors(
        plan,
        root,
        &paths,
        options,
        registry,
        || refuse_a_broken_baseline(root, plan, options, cancel),
    )?;
    // The checkpoint must agree with the journal either way. What translates anchors differs:
    // a journal that predates write-back left the plan in the coordinates the run began in, so the
    // journal's own fold does it; any other leaves the plan current, and what translates is this
    // run's edits alone.
    let folded = restore_ledger(&journal, &paths)?;
    let legacy = resume::predates_plan_write_back(&journal);
    let start = resume::start_of(&lowered, options, &journal)?;
    Ok(PlanRun {
        journal,
        plan: lowered,
        paths,
        ledger: if legacy {
            folded
        } else {
            PositionLedger::new()
        },
        legacy,
        start,
    })
}

/// Tell the progress sink what the run is about to do.
fn announce_run(options: &Options, total: usize, start: usize) {
    (options.progress)(&format!(
        "apply: {total} operation(s){}{}",
        if options.dry_run { ", dry-run" } else { "" },
        if start > 0 {
            format!(", from op {start}")
        } else {
            String::new()
        }
    ));
}

/// Whether `--stop-after` has been spent by the time the run reaches operation `index`, saying so
/// on the progress sink when it has.
fn stop_limit_reached(options: &Options, start: usize, index: usize) -> bool {
    let Some(limit) = options.stop_after else {
        return false;
    };
    if index < start + limit {
        return false;
    }
    (options.progress)(&format!(
        "stopped after {} operation(s) as requested",
        index - start
    ));
    true
}

/// Bring the plan `key` up to the tree after each of `settled`'s operations, in plan order.
fn refresh_plan(
    store: &mut PlanStore,
    key: &PlanKey,
    settled: &[(usize, Resolution)],
    registry: &mut BackendRegistry,
    journal: &mut Journal,
    paths: &StatePaths,
) -> Result<()> {
    for (member, applied) in settled {
        applied_op_record::record_applied_op(
            store, key, *member, applied, registry, journal, paths,
        )?;
    }
    Ok(())
}

/// Tell the account about each of `settled`'s operations as applied, oldest first, a group member's
/// line followed by the group it belongs to.
///
/// `done` counts everything applied so far, `settled` included.
fn report_settled(
    account: &ProgressSink,
    plan: &Plan,
    settled: &[(usize, Resolution)],
    done: usize,
) {
    let reported = done - settled.len();
    for (offset, (member, resolution)) in settled.iter().enumerate() {
        let op = &plan.ops[*member];
        if op.group.is_some() {
            report_visibility(account, resolution);
        }
        account(&progress_line(
            *member,
            reported + offset,
            plan.ops.len(),
            op.op,
            resolution.edit.changes.len(),
            true,
        ));
        if let Some(name) = &op.group {
            account(&crate::console::group(name));
        }
    }
}

fn apply_held_plan(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: &Options,
    client: Arc<LspClient>,
    cancel: &CancellationToken,
) -> Result<RunSummary> {
    let plan = store
        .get(key)
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
        })?
        .plan
        .clone();
    let mut registry = registry_for(
        client,
        cancel.clone(),
        Arc::clone(&options.progress),
        options.trace,
    );
    let PlanRun {
        mut journal,
        plan,
        paths,
        mut ledger,
        legacy,
        start,
    } = open_plan_run(
        &plan,
        &store.path_of(key),
        root,
        options,
        &mut registry,
        cancel,
    )?;
    let total = plan.ops.len();
    announce_run(options, total, start);
    let mut overlay = Overlay::new();
    let mut done = 0usize;
    let mut stopped_early = false;
    let mut group: Option<GroupRun> = None;

    let gate = GroupGate {
        root,
        paths: &paths,
        cancel,
    };
    // Any failure inside the loop ends the group it is in, whole: see
    // [`GroupRun::roll_back_on_failure`].
    let ran = (|| -> Result<()> {
        for (index, op) in plan.ops.iter().enumerate().skip(start) {
            // Honouring `--stop-after` is the run doing what it was told, so it ends the loop rather
            // than raising. Reporting it as a malformed plan — with a usage dump — described a
            // successful partial run as a defective one. Never inside a group, though: a group stands
            // or falls whole, so the limit is judged where a group would begin and its members all
            // count toward it.
            if group.is_none() && stop_limit_reached(options, start, index) {
                stopped_early = true;
                break;
            }

            let at = ledger.translate_op(op)?;
            (options.progress)(&format!(
                "op {index} of {total}: resolving {:?} in `{}`",
                op.op,
                at.anchor.file()
            ));
            let resolved = registry
                .backend_for(Path::new(at.anchor.file()), op.op)?
                .resolve(
                    &at,
                    &Workspace {
                        root,
                        overlay: &overlay,
                    },
                )?;

            // A group member's account waits for its group to be kept: see below.
            if op.group.is_none() || options.dry_run {
                report_visibility(&options.account, &resolved);
            }

            let files = resolved.edit.changes.len();
            (options.progress)(&format!("op {index} of {total}: resolved {files} file(s)"));
            if options.dry_run {
                (options.account)(&progress_line(
                    index,
                    done,
                    plan.ops.len(),
                    op.op,
                    files,
                    false,
                ));
                ledger.record(&resolved.edit);
                overlay.record(root, &resolved.edit)?;
                done += 1;
                continue;
            }

            (options.progress)(&format!(
                "op {index} of {total}: applying {files} file(s) to disk"
            ));
            let id = op.id.as_ref().filter(|_| !legacy);
            GroupRun::enter(&mut group, &plan, index, id, &resolved, &gate, &mut journal)?;
            commit_operation(
                index,
                id,
                &resolved,
                root,
                &paths,
                &mut journal,
                &mut ledger,
            )?;
            // A group's members reach the plan store together, once the group has compiled.
            let on_check = |name: &str| {
                (options.progress)(&format!(
                    "op {index} of {total}: checking group `{name}` compiles"
                ));
            };
            let settled =
                GroupRun::settle(&mut group, index, resolved, &gate, &mut journal, on_check)?;
            done += 1;
            let Settled::Ready(ready) = settled else {
                continue;
            };
            if !legacy {
                refresh_plan(store, key, &ready, &mut registry, &mut journal, &paths)?;
            }
            // Reported *after* the commit, so a line in the account means the edit is on disk and in
            // the journal — and, for a group member, that its group compiled: a member's line waits
            // for the group's end, so an edit a failed gate rolls back is never reported as applied.
            report_settled(&options.account, &plan, &ready, done);
        }
        Ok(())
    })();
    GroupRun::roll_back_on_failure(&mut group, ran, &gate, &mut journal)?;

    let run = AppliedRun {
        journal: &journal,
        paths: &paths,
        applied: done,
        total,
    };
    refuse_a_broken_result(root, options, run, cancel)?;
    Ok(RunSummary {
        applied: done,
        total: plan.ops.len(),
        stopped_early,
    })
}

mod applied_op_record;
pub use applied_op_record::*;

/// Refuse a run whose plan has a stale operation it will reach, before anything is read from the
/// tree or written.
///
/// Not only the *next* operation: a stale one further on is refused at the same place, since the
/// run would reach it after writing everything before it, and what it would do there is what its
/// anchor says about a tree that has moved on. The author re-anchors the operation; nothing here
/// re-targets it.
///
/// "Reach" is the run's own window: with `--stop-after N` the loop ends `N` operations after its
/// start, so a stale operation beyond that is not refused — the run never gets to it. A dry run
/// writes nothing, so what it would reach is not refused either.
///
/// What every apply loop calls first, the command line's and the daemon's.
///
/// # Errors
///
/// [`RestructureError::StaleOperation`] naming the first such operation and why it is stale.
pub fn refuse_a_stale_pending_op(
    store: &PlanStore,
    key: &PlanKey,
    options: &Options,
) -> Result<()> {
    if options.dry_run {
        return Ok(());
    }
    let stale = store.stale_ops(key);
    if stale.is_empty() {
        return Ok(());
    }
    let held = store.get(key).ok_or_else(|| {
        RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
    })?;
    let paths = StatePaths::for_plan(store.root(), &store.path_of(key))?;
    let journal = Journal::load(&paths.journal)?;
    let start = resume::start_of(&held.plan, options, &journal)?;
    let pending = held
        .plan
        .ops
        .iter()
        .skip(start)
        .take(options.stop_after.unwrap_or(usize::MAX))
        .filter_map(|op| op.id.as_ref());
    for id in pending {
        if let Some(found) = stale.iter().find(|found| &found.op == id) {
            return Err(RestructureError::StaleOperation {
                plan: key.to_string(),
                op: id.to_string(),
                reason: found.reason.to_string(),
            });
        }
    }
    Ok(())
}

/// What a check reports of `plan`'s stale operations: one finding each, at the operation's index.
#[must_use]
pub fn stale_findings(plan: &Plan, stale: &[crate::plan_store::OpStaleness]) -> Vec<Finding> {
    stale
        .iter()
        .filter_map(|found| {
            let operation = plan
                .ops
                .iter()
                .position(|op| op.id.as_ref() == Some(&found.op))?;
            Some(Finding {
                operation,
                detail: format!("stale: {} — re-anchor it before applying", found.reason),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::edit::{FileEdit, Position, TextEdit, WorkspaceEdit};
    use crate::item_anchor::{ItemResolver, ResolvedItem};
    use crate::plan::ItemPath;
    use crate::plan_store::FlushPolicy;
    use crate::{JournalRecord, OpId, Range, Resolution};

    const FIRST: &str = r#"{"id":"a1","op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":2,"col":2}},"name":"f"}"#;

    /// A plan of three operations, `c1`, `c2` and `c3`, anchored at lines 10–11, 20–21 and 30–31.
    fn three_range_ops() -> String {
        (1..=3)
            .map(|n| {
                format!(
                    r#"{{"id":"c{n}","op":"extract_method","anchor":{{"kind":"range","file":"src/a.rs","start":{{"line":{},"col":5}},"end":{{"line":{},"col":6}}}},"name":"f{n}"}}"#,
                    n * 10,
                    n * 10 + 1
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn a_workspace_holding(plans: &[(&str, String)]) -> (tempfile::TempDir, PlanStore) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/a.rs"), "fn untouched() {}\n").unwrap();
        for (name, ops) in plans {
            std::fs::write(
                root.path().join(name),
                format!("{{\"v\":1,\"snapshot\":{{}}}}\n{ops}\n"),
            )
            .unwrap();
        }
        let mut store = PlanStore::new(
            root.path(),
            FlushPolicy {
                debounce: Duration::from_secs(3600),
            },
        );
        let named: Vec<PathBuf> = plans.iter().map(|(name, _)| PathBuf::from(name)).collect();
        store.load(&named).unwrap();
        (root, store)
    }

    fn key(store: &PlanStore, name: &str) -> PlanKey {
        store.key_for(Path::new(name)).unwrap()
    }

    fn lines_replaced(first: u32, last: u32, new_text: &str) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: "src/a.rs".to_string(),
                edits: vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: first,
                            col: 1,
                        },
                        end: Position { line: last, col: 1 },
                    },
                    new_text: new_text.to_string(),
                }],
            }],
        }
    }

    /// `second.jsonl`'s `c1`–`c3` held beside `first.jsonl`, with the operation `stale` of the
    /// second made stale by an operation of the first that replaced the lines it is anchored at.
    fn second_with_stale(stale: usize) -> (tempfile::TempDir, PlanStore, PlanKey) {
        let (root, mut store) = a_workspace_holding(&[
            ("first.jsonl", FIRST.to_string()),
            ("second.jsonl", three_range_ops()),
        ]);
        let (first, second) = (key(&store, "first.jsonl"), key(&store, "second.jsonl"));
        let at = (stale as u32 + 1) * 10;
        store
            .fold_foreign_op(
                &first,
                &OpId("a1".to_string()),
                &lines_replaced(at, at + 1, "    helper();\n"),
                &mut ARefusingResolver,
            )
            .unwrap();
        (root, store, second)
    }

    /// A resolver whose every answer is a server failure — not about the item, so a fold returns it
    /// instead of leaving the anchor as written.
    struct ARefusingResolver;

    impl ItemResolver for ARefusingResolver {
        fn resolve_item(&mut self, _file: &str, _item: &ItemPath) -> Result<ResolvedItem> {
            Err(RestructureError::ServerDefect("the server died".into()))
        }
    }

    fn refused(
        store: &PlanStore,
        key: &PlanKey,
        options: &Options,
    ) -> std::result::Result<(), String> {
        refuse_a_stale_pending_op(store, key, options).map_err(|error| error.to_string())
    }

    #[test]
    fn a_stale_operation_past_where_stop_after_ends_the_run_is_not_refused() {
        // Given c3 stale, and a run that stops after its first operation
        let (_root, store, second) = second_with_stale(2);
        let options = Options {
            stop_after: Some(1),
            ..Options::default()
        };

        // When the run is checked for stale operations
        let refusal = refused(&store, &second, &options);

        // Then it is not refused: the run never reaches c3
        assert_eq!(refusal, Ok(()));
    }

    #[test]
    fn a_stale_operation_inside_the_window_stop_after_leaves_is_refused_naming_it() {
        // Given c2 stale, and a run that stops after two operations
        let (_root, store, second) = second_with_stale(1);
        let options = Options {
            stop_after: Some(2),
            ..Options::default()
        };

        // When the run is checked for stale operations
        let refusal = refused(&store, &second, &options);

        // Then it is refused naming the plan, the operation and why
        assert_eq!(
            refusal,
            Err(
                "operation `c2` of second.jsonl is stale (edited by first.jsonl#a1) — re-anchor \
                 it before applying"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_dry_run_with_a_stale_operation_is_not_refused() {
        // Given c1 stale, and a dry run, which writes nothing
        let (_root, store, second) = second_with_stale(0);
        let options = Options {
            dry_run: true,
            ..Options::default()
        };

        // When the run is checked for stale operations
        let refusal = refused(&store, &second, &options);

        // Then it is not refused
        assert_eq!(refusal, Ok(()));
    }

    #[test]
    fn a_stale_operation_that_is_not_the_next_one_is_refused_before_anything_is_written() {
        // Given c3 stale in a run that starts at c1
        let (root, store, second) = second_with_stale(2);
        let plan_before = std::fs::read(store.path_of(&second)).unwrap();

        // When the run is checked for stale operations
        let refusal = refused(&store, &second, &Options::default());

        // Then it is refused, and neither the tree, the plan file nor a journal was touched
        assert_eq!(
            refusal,
            Err(
                "operation `c3` of second.jsonl is stale (edited by first.jsonl#a1) — re-anchor \
                 it before applying"
                    .to_string()
            )
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("src/a.rs")).unwrap(),
            "fn untouched() {}\n"
        );
        assert_eq!(std::fs::read(store.path_of(&second)).unwrap(), plan_before);
        assert!(!root.path().join(".restructure").exists());
    }

    #[test]
    fn a_failure_folding_another_plan_leaves_the_applying_plan_resumable() {
        // Given the applying plan `first`, whose second operation is a range anchor the edit moves,
        // and `second`, whose item anchor is in the file the edit changes
        let item = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:written","hint":{"line":20,"col":5}},"name":"g"}"#;
        let pending = r#"{"id":"a2","op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":20,"col":5},"end":{"line":22,"col":6}},"name":"h"}"#;
        let (root, mut store) = a_workspace_holding(&[
            ("first.jsonl", format!("{FIRST}\n{pending}")),
            ("second.jsonl", item.to_string()),
        ]);
        let first = key(&store, "first.jsonl");
        let paths = StatePaths::for_plan(root.path(), &store.path_of(&first)).unwrap();
        let edit = lines_replaced(1, 1, "// one\n// two\n// three\n");
        let mut journal = Journal::default();
        journal
            .append(
                &paths.journal,
                JournalRecord::completed(
                    0,
                    Some(OpId("a1".to_string())),
                    edit.clone(),
                    Default::default(),
                    Default::default(),
                    Vec::new(),
                    Vec::new(),
                ),
            )
            .unwrap();

        // When the operation is recorded, and the server fails while the other plan is folded
        let recorded = applied_op_record::record_applied_op(
            &mut store,
            &first,
            0,
            &Resolution::of(edit),
            &mut ARefusingResolver,
            &mut journal,
            &paths,
        );

        // Then the run fails loudly
        assert_eq!(
            recorded.map_err(|error| error.to_string()),
            Err("rust-analyzer's answer was unusable: the server died".to_string())
        );
        // And the plan on disk is the one the journal vouches for, so a resume is not refused
        let on_disk =
            Plan::parse(&std::fs::read_to_string(store.path_of(&first)).unwrap()).unwrap();
        let journal = Journal::load(&paths.journal).unwrap();
        assert_eq!(
            resume::refuse_a_plan_the_journal_cannot_vouch_for(&on_disk, &journal)
                .map_err(|error| error.to_string()),
            Ok(())
        );
    }
}
