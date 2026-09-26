//! The plan store: plans loaded into memory once, executed by reference, and written back.
//!
//! A plan file used to be re-read by every `check`, `apply` and `status`, so what executed was
//! whatever the file said at that moment — including anchors that earlier operations of the same
//! plan had already moved past. The store reads a plan once, indexes it by path and by operation
//! id, and is what the executor reads from. After each applied operation it rewrites that plan's
//! pending anchors, and it writes changed plans back: shortly after they change, and always at the
//! end of a run, on unload and on shutdown.
//!
//! One type for both lifetimes. `tddy-index-daemon` keeps a store per workspace root across
//! requests; a one-shot `tddy-tools restructure apply` keeps one for the length of the run. The
//! plan on disk stays the source a human edits and reviews — the store is a cache that writes back,
//! and it refuses to overwrite a file somebody changed underneath it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::edit::WorkspaceEdit;
use crate::item_anchor::ItemResolver;
use crate::plan::{OpId, Plan, RefactorOp};
use crate::Result;

/// A loaded plan's identity: its path relative to the workspace root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanKey(pub PathBuf);

/// When a changed plan is written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushPolicy {
    /// How long a plan may stay dirty before a flush writes it: eventual, not per-edit.
    pub debounce: Duration,
}

/// A plan the store holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPlan {
    pub key: PlanKey,
    pub plan: Plan,
    /// Whether the store's copy differs from what it last read or wrote.
    pub dirty: bool,
    /// The file's hash when the store last read or wrote it — what a flush checks before writing.
    pub on_disk: String,
}

/// What `LoadPlans` and `ListPlans` report per plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSummary {
    pub key: PlanKey,
    pub ops: usize,
    pub dirty: bool,
}

/// Why a held operation can no longer be run as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleReason {
    /// Its item was found, and its text is no longer what the anchor was written against.
    ItemChanged,
    /// Its item is not declared in the file the anchor names any more.
    ItemNotFound { file: String },
    /// An operation of another loaded plan edited inside its anchored range.
    EditedBy { plan: PlanKey, op: OpId },
}

impl std::fmt::Display for StaleReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StaleReason::ItemChanged => formatter.write_str("item changed"),
            StaleReason::ItemNotFound { file } => write!(formatter, "item not found in {file}"),
            StaleReason::EditedBy { plan, op } => {
                write!(formatter, "edited by {}#{op}", plan.0.display())
            }
        }
    }
}

/// One held operation that is stale, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpStaleness {
    pub op: OpId,
    pub reason: StaleReason,
}

/// Every plan loaded under one workspace root.
#[derive(Debug)]
pub struct PlanStore {
    root: PathBuf,
    policy: FlushPolicy,
    plans: BTreeMap<PlanKey, LoadedPlan>,
}

impl PlanStore {
    pub fn new(root: &Path, policy: FlushPolicy) -> PlanStore {
        PlanStore {
            root: root.to_path_buf(),
            policy,
            plans: BTreeMap::new(),
        }
    }

    /// Read each plan, assign an id to every operation without one, and hold it.
    ///
    /// Two operations sharing an id are refused as malformed. Loading a plan already held reloads
    /// nothing and reports it as held.
    pub fn load(&mut self, plans: &[PathBuf]) -> Result<Vec<PlanSummary>> {
        // TODO(plan-store): implement
        let _ = (plans, &self.root, self.policy);
        todo!("plan-store: load plans")
    }

    /// Flush and drop each named plan.
    pub fn unload(&mut self, plans: &[PathBuf]) -> Result<()> {
        // TODO(plan-store): implement
        let _ = plans;
        todo!("plan-store: unload plans")
    }

    /// Flush and drop every plan this root holds.
    pub fn unload_all(&mut self) -> Result<()> {
        // TODO(plan-store): implement
        todo!("plan-store: unload every plan")
    }

    /// Every plan held, in path order.
    pub fn list(&self) -> Vec<PlanSummary> {
        // TODO(plan-store): implement
        todo!("plan-store: list plans")
    }

    /// The key a plan path is held under, relative to the root whatever form it was named in.
    pub fn key_for(&self, plan: &Path) -> Result<PlanKey> {
        // TODO(plan-store): implement
        let _ = plan;
        todo!("plan-store: a plan path's key")
    }

    pub fn get(&self, key: &PlanKey) -> Option<&LoadedPlan> {
        self.plans.get(key)
    }

    /// One operation of a held plan, by its id.
    pub fn op(&self, key: &PlanKey, id: &OpId) -> Option<&RefactorOp> {
        // TODO(plan-store): implement
        let _ = (key, id);
        todo!("plan-store: an operation by id")
    }

    /// Rewrite the pending operations of `key` after operation `applied` produced `edit`: hints and
    /// relative ranges translated through it, fingerprints of items it edited recomputed through
    /// `resolver`, `file` hints following a file it moved. Marks the plan dirty.
    pub fn refresh_after_op(
        &mut self,
        key: &PlanKey,
        applied: &OpId,
        edit: &WorkspaceEdit,
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        // TODO(plan-store): implement
        let _ = (key, applied, edit, resolver);
        todo!("plan-store: refresh the applied plan's pending operations")
    }

    /// Fold operation `op` of plan `from`, which produced `edit`, into **every other** held plan:
    /// their anchors translated through it, their `file` hints following a moved file, their
    /// fingerprints recomputed where the edit touched an item outside the anchored range. An edit
    /// overlapping another plan's anchored range marks that operation stale.
    pub fn fold_foreign_op(
        &mut self,
        from: &PlanKey,
        op: &OpId,
        edit: &WorkspaceEdit,
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        // TODO(live-plans): implement
        let _ = (from, op, edit, resolver);
        todo!("live-plans: fold an applied operation into every other held plan")
    }

    /// Re-resolve every held item anchor in `files` after they changed underneath the store: hints
    /// and per-file hints rewritten where the item is intact, the operation stale where it is not.
    pub fn reresolve_files(
        &mut self,
        files: &[String],
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        // TODO(live-plans): implement
        let _ = (files, resolver);
        todo!("live-plans: re-resolve anchors in files changed underneath the store")
    }

    /// The stale operations of a held plan, in plan order.
    pub fn stale_ops(&self, key: &PlanKey) -> Vec<OpStaleness> {
        // TODO(live-plans): implement
        let _ = key;
        todo!("live-plans: a held plan's stale operations")
    }

    /// Write every plan dirty for longer than the debounce; the keys written.
    pub fn flush_dirty(&mut self) -> Result<Vec<PlanKey>> {
        // TODO(plan-store): implement
        todo!("plan-store: eventual flush")
    }

    /// Write every dirty plan now — end of run, unload, shutdown; the keys written.
    pub fn flush_all(&mut self) -> Result<Vec<PlanKey>> {
        // TODO(plan-store): implement
        todo!("plan-store: flush every dirty plan")
    }
}

/// Re-resolve the item anchors of the plan file at `plan` once against the tree under `root` and
/// write it back — what `restructure snapshot` does for a plan no daemon holds.
///
/// Operations whose item changed are reported and left as they are.
pub fn rebase_plan_file(
    root: &Path,
    plan: &Path,
    resolver: &mut dyn ItemResolver,
) -> Result<Vec<OpStaleness>> {
    // TODO(live-plans): implement
    let _ = (root, plan, resolver);
    todo!("live-plans: re-resolve and rewrite one plan file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Position, TextEdit};
    use crate::item_anchor::ResolvedItem;
    use crate::plan::{Anchor, Fingerprint, ItemPath};
    use crate::Range;

    const HEADER: &str = r#"{"v":1,"snapshot":{}}"#;

    /// A resolver that finds every item at the same place, fingerprinted as `fingerprint` — what a
    /// test of the store's bookkeeping needs, without a language server.
    struct AnItemResolverAnswering {
        fingerprint: &'static str,
    }

    impl ItemResolver for AnItemResolverAnswering {
        fn resolve_item(&mut self, _file: &str, _item: &ItemPath) -> Result<ResolvedItem> {
            Ok(ResolvedItem {
                range: Range {
                    start: Position { line: 4, col: 1 },
                    end: Position { line: 9, col: 2 },
                },
                name: Position { line: 4, col: 8 },
                fingerprint: Fingerprint(self.fingerprint.to_string()),
            })
        }
    }

    fn a_store_holding(lines: &[&str]) -> (tempfile::TempDir, PlanStore, PlanKey) {
        let root = tempfile::tempdir().unwrap();
        let mut text = vec![HEADER.to_string()];
        text.extend(lines.iter().map(|line| line.to_string()));
        std::fs::write(root.path().join("plan.jsonl"), text.join("\n") + "\n").unwrap();
        let mut store = PlanStore::new(
            root.path(),
            FlushPolicy {
                debounce: Duration::from_secs(3600),
            },
        );
        store.load(&[PathBuf::from("plan.jsonl")]).unwrap();
        let key = store.key_for(Path::new("plan.jsonl")).unwrap();
        (root, store, key)
    }

    fn three_lines_inserted_at_the_top_of(path: &str) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: path.to_string(),
                edits: vec![TextEdit {
                    range: Range {
                        start: Position { line: 1, col: 1 },
                        end: Position { line: 1, col: 1 },
                    },
                    new_text: "// one\n// two\n// three\n".to_string(),
                }],
            }],
        }
    }

    fn the_anchor_of(store: &PlanStore, key: &PlanKey, index: usize) -> Anchor {
        store.get(key).unwrap().plan.ops[index].anchor.clone()
    }

    fn the_id_of(store: &PlanStore, key: &PlanKey, index: usize) -> OpId {
        store.get(key).unwrap().plan.ops[index].id.clone().unwrap()
    }

    #[test]
    fn a_pending_range_anchor_moves_down_past_lines_an_applied_op_inserted_above_it() {
        // Given two ops in one file; the second anchored at lines 20–22
        let (_root, mut store, key) = a_store_holding(&[
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":20,"col":5},"end":{"line":22,"col":6}},"name":"f"}"#,
        ]);
        let first = the_id_of(&store, &key, 0);

        // When the first op's edit inserted three lines at the top
        store
            .refresh_after_op(
                &key,
                &first,
                &three_lines_inserted_at_the_top_of("src/a.rs"),
                &mut AnItemResolverAnswering {
                    fingerprint: "sha256:unused",
                },
            )
            .unwrap();

        // Then the second op is anchored three lines lower and the plan is dirty
        assert_eq!(
            the_anchor_of(&store, &key, 1),
            Anchor::Range {
                file: "src/a.rs".to_string(),
                start: Position { line: 23, col: 5 },
                end: Position { line: 25, col: 6 },
            }
        );
        assert!(store.get(&key).unwrap().dirty);
    }

    #[test]
    fn a_pending_item_anchors_hint_and_fingerprint_follow_an_edit_to_its_item() {
        // Given a pending item anchor whose hint is line 20
        let (_root, mut store, key) = a_store_holding(&[
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            r#"{"op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:before","hint":{"line":20,"col":5}},"name":"g"}"#,
        ]);
        let first = the_id_of(&store, &key, 0);

        // When the first op's edit touched the item, which now resolves with a new fingerprint
        store
            .refresh_after_op(
                &key,
                &first,
                &three_lines_inserted_at_the_top_of("src/a.rs"),
                &mut AnItemResolverAnswering {
                    fingerprint: "sha256:after",
                },
            )
            .unwrap();

        // Then the anchor carries the item's new fingerprint and a hint where it now starts
        assert_eq!(
            the_anchor_of(&store, &key, 1),
            Anchor::Item {
                item: ItemPath::parse("a::f").unwrap(),
                file: "src/a.rs".to_string(),
                start: Some(Position { line: 2, col: 5 }),
                end: Some(Position { line: 3, col: 6 }),
                fingerprint: Fingerprint("sha256:after".to_string()),
                hint: Some(Position { line: 5, col: 5 }),
            }
        );
    }

    #[test]
    fn a_pending_anchor_follows_a_file_the_applied_op_moved() {
        // Given a pending symbol anchor in `src/a.rs`
        let (_root, mut store, key) = a_store_holding(&[
            r#"{"op":"extract_module_to_file","anchor":{"kind":"symbol","file":"src/a.rs","path":"inner"}}"#,
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"C"},"name":"D"}"#,
        ]);
        let first = the_id_of(&store, &key, 0);

        // When the first op renamed the file
        store
            .refresh_after_op(
                &key,
                &first,
                &WorkspaceEdit {
                    changes: vec![FileEdit::Rename {
                        from: "src/a.rs".to_string(),
                        to: "src/a/mod.rs".to_string(),
                    }],
                },
                &mut AnItemResolverAnswering {
                    fingerprint: "sha256:unused",
                },
            )
            .unwrap();

        // Then the second op names the file where it now is
        assert_eq!(the_anchor_of(&store, &key, 1).file(), "src/a/mod.rs");
    }

    #[test]
    fn flush_dirty_leaves_a_plan_dirty_for_less_than_the_debounce() {
        // Given a freshly loaded plan whose ops were just given ids, and an hour's debounce
        let (root, mut store, key) = a_store_holding(&[
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
        ]);
        let before = std::fs::read_to_string(root.path().join("plan.jsonl")).unwrap();

        // When an eventual flush runs
        let written = store.flush_dirty().unwrap();

        // Then nothing was written yet
        assert_eq!(written, Vec::<PlanKey>::new());
        assert_eq!(
            std::fs::read_to_string(root.path().join("plan.jsonl")).unwrap(),
            before
        );
        assert!(store.get(&key).unwrap().dirty);
    }

    #[test]
    fn loading_a_plan_already_held_keeps_the_held_copy() {
        // Given a held plan whose pending op was refreshed in memory
        let (_root, mut store, key) = a_store_holding(&[
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":20,"col":5},"end":{"line":22,"col":6}},"name":"f"}"#,
        ]);
        let first = the_id_of(&store, &key, 0);
        store
            .refresh_after_op(
                &key,
                &first,
                &three_lines_inserted_at_the_top_of("src/a.rs"),
                &mut AnItemResolverAnswering {
                    fingerprint: "sha256:unused",
                },
            )
            .unwrap();

        // When it is loaded again
        store.load(&[PathBuf::from("plan.jsonl")]).unwrap();

        // Then the refreshed copy is still the one held
        assert_eq!(
            the_anchor_of(&store, &key, 1)
                .as_range()
                .unwrap()
                .start
                .line,
            23
        );
    }
}

#[cfg(test)]
mod live_plans_tests {
    use super::*;
    use crate::edit::{FileEdit, Position, TextEdit};
    use crate::item_anchor::ResolvedItem;
    use crate::plan::{Anchor, Fingerprint, ItemPath};
    use crate::{Range, RestructureError};

    /// A resolver whose answer the test chooses: the item where it is, fingerprinted as given, or
    /// not there at all.
    enum AResolverAnswering {
        Found {
            first_line: u32,
            fingerprint: &'static str,
        },
        Missing,
    }

    impl ItemResolver for AResolverAnswering {
        fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
            match self {
                AResolverAnswering::Found {
                    first_line,
                    fingerprint,
                } => Ok(ResolvedItem {
                    range: Range {
                        start: Position {
                            line: *first_line,
                            col: 1,
                        },
                        end: Position {
                            line: *first_line + 5,
                            col: 2,
                        },
                    },
                    name: Position {
                        line: *first_line,
                        col: 8,
                    },
                    fingerprint: Fingerprint(fingerprint.to_string()),
                }),
                AResolverAnswering::Missing => Err(RestructureError::MalformedPlan(format!(
                    "`{item}` is not declared in {file}"
                ))),
            }
        }
    }

    const FIRST: &str = r#"{"id":"a1","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#;
    const SECOND_BY_RANGE: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":20,"col":5},"end":{"line":22,"col":6}},"name":"f"}"#;
    const SECOND_BY_ITEM: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:written","hint":{"line":20,"col":5}},"name":"g"}"#;

    fn two_held_plans(second: &str) -> (tempfile::TempDir, PlanStore, PlanKey, PlanKey) {
        let root = tempfile::tempdir().unwrap();
        for (name, op) in [("first.jsonl", FIRST), ("second.jsonl", second)] {
            std::fs::write(
                root.path().join(name),
                format!("{{\"v\":1,\"snapshot\":{{}}}}\n{op}\n"),
            )
            .unwrap();
        }
        let mut store = PlanStore::new(
            root.path(),
            FlushPolicy {
                debounce: Duration::from_secs(3600),
            },
        );
        store
            .load(&[PathBuf::from("first.jsonl"), PathBuf::from("second.jsonl")])
            .unwrap();
        let first = store.key_for(Path::new("first.jsonl")).unwrap();
        let second = store.key_for(Path::new("second.jsonl")).unwrap();
        (root, store, first, second)
    }

    fn an_edit_of(path: &str, first: u32, last: u32, new_text: &str) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: path.to_string(),
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

    fn the_only_anchor_of(store: &PlanStore, key: &PlanKey) -> Anchor {
        store.get(key).unwrap().plan.ops[0].anchor.clone()
    }

    fn a1() -> OpId {
        OpId("a1".to_string())
    }

    #[test]
    fn a_foreign_op_moves_another_plans_range_anchor_down_past_lines_it_inserted() {
        // Given plan `second` anchored at lines 20–22
        let (_root, mut store, first, second) = two_held_plans(SECOND_BY_RANGE);

        // When `first`'s op inserted three lines at the top of the file
        store
            .fold_foreign_op(
                &first,
                &a1(),
                &an_edit_of("src/a.rs", 1, 1, "// one\n// two\n// three\n"),
                &mut AResolverAnswering::Missing,
            )
            .unwrap();

        // Then `second`'s op is anchored three lines lower, and it is not stale
        assert_eq!(
            the_only_anchor_of(&store, &second),
            Anchor::Range {
                file: "src/a.rs".to_string(),
                start: Position { line: 23, col: 5 },
                end: Position { line: 25, col: 6 },
            }
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn a_foreign_op_editing_inside_another_plans_range_marks_it_edited_by() {
        // Given plan `second` anchored at lines 20–22
        let (_root, mut store, first, second) = two_held_plans(SECOND_BY_RANGE);

        // When `first`'s op replaced lines 21–22
        store
            .fold_foreign_op(
                &first,
                &a1(),
                &an_edit_of("src/a.rs", 21, 23, "    helper();\n"),
                &mut AResolverAnswering::Missing,
            )
            .unwrap();

        // Then `second`'s op is stale, naming the op that edited it
        assert_eq!(
            store.stale_ops(&second),
            vec![OpStaleness {
                op: OpId("b1".to_string()),
                reason: StaleReason::EditedBy {
                    plan: first.clone(),
                    op: a1(),
                },
            }]
        );
    }

    #[test]
    fn a_foreign_file_move_moves_another_plans_file_hint() {
        // Given plan `second` anchored in `src/a.rs`
        let (_root, mut store, first, second) = two_held_plans(SECOND_BY_RANGE);

        // When `first`'s op moved the file
        store
            .fold_foreign_op(
                &first,
                &a1(),
                &WorkspaceEdit {
                    changes: vec![FileEdit::Rename {
                        from: "src/a.rs".to_string(),
                        to: "src/a/mod.rs".to_string(),
                    }],
                },
                &mut AResolverAnswering::Missing,
            )
            .unwrap();

        // Then `second` names the file where it now is
        assert_eq!(the_only_anchor_of(&store, &second).file(), "src/a/mod.rs");
    }

    #[test]
    fn a_foreign_op_leaves_the_plan_it_came_from_alone() {
        // Given plan `first`, whose own op is being folded
        let (_root, mut store, first, _second) = two_held_plans(SECOND_BY_RANGE);
        let before = the_only_anchor_of(&store, &first);

        // When its own op is folded as a foreign one
        store
            .fold_foreign_op(
                &first,
                &a1(),
                &an_edit_of("src/a.rs", 1, 1, "// one\n"),
                &mut AResolverAnswering::Missing,
            )
            .unwrap();

        // Then `first` itself is untouched — its own refresh is `refresh_after_op`'s
        assert_eq!(the_only_anchor_of(&store, &first), before);
    }

    #[test]
    fn re_resolving_an_intact_item_rewrites_only_its_hint() {
        // Given an item anchor hinted at line 20, whose item now starts at line 23 unchanged
        let (_root, mut store, _first, second) = two_held_plans(SECOND_BY_ITEM);

        // When its file changed underneath the store
        store
            .reresolve_files(
                &["src/a.rs".to_string()],
                &mut AResolverAnswering::Found {
                    first_line: 23,
                    fingerprint: "sha256:written",
                },
            )
            .unwrap();

        // Then the hint follows the item, the fingerprint is unchanged, and nothing is stale
        let Anchor::Item {
            hint, fingerprint, ..
        } = the_only_anchor_of(&store, &second)
        else {
            panic!("the item anchor stayed an item anchor");
        };
        assert_eq!(hint, Some(Position { line: 24, col: 5 }));
        assert_eq!(fingerprint, Fingerprint("sha256:written".to_string()));
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn re_resolving_a_changed_item_marks_its_op_item_changed() {
        let (_root, mut store, _first, second) = two_held_plans(SECOND_BY_ITEM);

        store
            .reresolve_files(
                &["src/a.rs".to_string()],
                &mut AResolverAnswering::Found {
                    first_line: 20,
                    fingerprint: "sha256:edited",
                },
            )
            .unwrap();

        assert_eq!(
            store.stale_ops(&second),
            vec![OpStaleness {
                op: OpId("b1".to_string()),
                reason: StaleReason::ItemChanged,
            }]
        );
    }

    #[test]
    fn re_resolving_an_item_that_is_gone_marks_its_op_item_not_found() {
        let (_root, mut store, _first, second) = two_held_plans(SECOND_BY_ITEM);

        store
            .reresolve_files(&["src/a.rs".to_string()], &mut AResolverAnswering::Missing)
            .unwrap();

        assert_eq!(
            store.stale_ops(&second),
            vec![OpStaleness {
                op: OpId("b1".to_string()),
                reason: StaleReason::ItemNotFound {
                    file: "src/a.rs".to_string()
                },
            }]
        );
    }

    #[test]
    fn a_stale_reason_reads_the_way_the_wire_reports_it() {
        assert_eq!(
            StaleReason::EditedBy {
                plan: PlanKey(PathBuf::from("first.jsonl")),
                op: a1(),
            }
            .to_string(),
            "edited by first.jsonl#a1"
        );
    }
}
