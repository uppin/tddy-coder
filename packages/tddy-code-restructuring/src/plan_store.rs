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
