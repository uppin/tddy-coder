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
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

#[cfg(test)]
use crate::edit::FileEdit;
use crate::edit::WorkspaceEdit;
use crate::item_anchor::ItemResolver;
use crate::plan::{OpId, Plan, RefactorOp};
use crate::{RestructureError, Result};

/// A loaded plan's identity: its path relative to the workspace root.
///
/// A plan outside the root has no relative path to be known by, so it is keyed by its absolute one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanKey(pub PathBuf);

impl std::fmt::Display for PlanKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(formatter)
    }
}

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
    /// When each dirty plan first became dirty — what the debounce is measured from. Kept beside
    /// the plans rather than in them so [`LoadedPlan`] stays what a reader of the store sees.
    dirty_since: BTreeMap<PlanKey, Instant>,
    /// Which operations are stale and which files the store's own runs wrote — derived state, kept
    /// out of the plans so a plan written back is still just a plan.
    live: live::Liveness,
}

impl PlanStore {
    pub fn new(root: &Path, policy: FlushPolicy) -> PlanStore {
        PlanStore {
            root: root.to_path_buf(),
            policy,
            plans: BTreeMap::new(),
            dirty_since: BTreeMap::new(),
            live: live::Liveness::default(),
        }
    }

    /// The workspace root the plans are keyed against.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Read each plan, assign an id to every operation without one, and hold it.
    ///
    /// Two operations sharing an id are refused as malformed. Loading a plan already held reloads
    /// nothing and reports it as held. All or nothing: when one plan is refused, none of the others
    /// named is loaded either, so a failed `load` leaves the store as it found it.
    pub fn load(&mut self, plans: &[PathBuf]) -> Result<Vec<PlanSummary>> {
        let mut named = Vec::new();
        let mut read: BTreeMap<PlanKey, LoadedPlan> = BTreeMap::new();
        for plan in plans {
            let key = self.key_for(plan)?;
            if !self.plans.contains_key(&key) && !read.contains_key(&key) {
                read.insert(key.clone(), self.read(&key)?);
            }
            named.push(key);
        }

        for (key, loaded) in read {
            if loaded.dirty {
                self.dirty_since.insert(key.clone(), Instant::now());
            }
            self.plans.insert(key, loaded);
        }
        Ok(named
            .into_iter()
            .filter_map(|key| self.plans.get(&key).map(summary_of))
            .collect())
    }

    /// Flush and drop each named plan.
    ///
    /// A plan whose file changed on disk since it was loaded is dropped without being written: the
    /// file is the human's source, and "unload it and load it again" is how a refused flush tells
    /// them to take theirs over the store's. Every other failure to write keeps the plan held.
    /// Naming a plan the store does not hold is refused, before anything is dropped.
    pub fn unload(&mut self, plans: &[PathBuf]) -> Result<()> {
        let keys = plans
            .iter()
            .map(|plan| {
                let key = self.key_for(plan)?;
                self.held(&key).map(|_| key)
            })
            .collect::<Result<Vec<_>>>()?;
        for key in keys {
            self.release(&key)?;
        }
        Ok(())
    }

    /// Flush and drop every plan this root holds.
    ///
    /// Every plan is tried even when one fails to be written; the first failure is returned.
    pub fn unload_all(&mut self) -> Result<()> {
        let keys: Vec<PlanKey> = self.plans.keys().cloned().collect();
        let mut first_failure = None;
        for key in keys {
            if let Err(failure) = self.release(&key) {
                first_failure.get_or_insert(failure);
            }
        }
        first_failure.map_or(Ok(()), Err)
    }

    /// Every plan held, in path order.
    pub fn list(&self) -> Vec<PlanSummary> {
        self.plans.values().map(summary_of).collect()
    }

    /// The key a plan path is held under, relative to the root whatever form it was named in.
    ///
    /// Lexical, never canonical: the file need not exist to have a key, and `./a/../plan.jsonl`,
    /// `plan.jsonl` and `<root>/plan.jsonl` are one plan.
    pub fn key_for(&self, plan: &Path) -> Result<PlanKey> {
        let normalised = normalised(plan);
        let key = normalised
            .strip_prefix(&self.root)
            .map(Path::to_path_buf)
            .unwrap_or(normalised);
        if key.file_name().is_none() {
            return Err(RestructureError::MalformedPlan(format!(
                "`{}` names no plan file",
                plan.display()
            )));
        }
        Ok(PlanKey(key))
    }

    /// Where the file a key names is, on disk.
    pub fn path_of(&self, key: &PlanKey) -> PathBuf {
        self.root.join(&key.0)
    }

    pub fn get(&self, key: &PlanKey) -> Option<&LoadedPlan> {
        self.plans.get(key)
    }

    /// One operation of a held plan, by its id.
    pub fn op(&self, key: &PlanKey, id: &OpId) -> Option<&RefactorOp> {
        self.get(key)?
            .plan
            .ops
            .iter()
            .find(|op| op.id.as_ref() == Some(id))
    }

    /// Rewrite the pending operations of `key` after operation `applied` produced `edit`: hints and
    /// relative ranges translated through it, fingerprints of items it edited recomputed through
    /// `resolver`, `file` hints following a file it moved. Marks the plan dirty.
    ///
    /// Pending means after `applied` in the plan's order. The plan is changed only once every
    /// pending operation has been refreshed, so a refusal part-way leaves it as it was.
    ///
    /// An item the edit left unresolvable — moved out of its file, say — keeps its anchor as
    /// written: whether an operation is still valid is judged when it runs, not here.
    pub fn refresh_after_op(
        &mut self,
        key: &PlanKey,
        applied: &OpId,
        edit: &WorkspaceEdit,
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        let held = self.held(key)?;
        let position = held
            .plan
            .ops
            .iter()
            .position(|op| op.id.as_ref() == Some(applied))
            .ok_or_else(|| {
                RestructureError::MalformedPlan(format!(
                    "{key} has no operation with the id `{applied}`"
                ))
            })?;

        let mut refreshed = held.plan.ops.clone();
        let changed = refresh::refresh_pending(&mut refreshed[position + 1..], edit, resolver)?;
        if changed {
            if let Some(held) = self.plans.get_mut(key) {
                held.plan.ops = refreshed;
            }
            self.mark_dirty(key);
        }
        Ok(())
    }

    /// Fold operation `op` of plan `from`, which produced `edit`, into **every other** held plan:
    /// their anchors translated through it, their `file` hints following a moved file, their
    /// fingerprints recomputed where the edit touched an item outside the anchored range. An edit
    /// overlapping another plan's anchored range marks that operation stale.
    ///
    /// Nothing is changed unless every plan could be folded. The plan `from` is left to
    /// [`Self::refresh_after_op`], and a plan nobody loaded is not here to be touched.
    pub fn fold_foreign_op(
        &mut self,
        from: &PlanKey,
        op: &OpId,
        edit: &WorkspaceEdit,
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        live::fold(self, from, op, edit, resolver)
    }

    /// Re-resolve every held item anchor in `files` after they changed underneath the store: hints
    /// and per-file hints rewritten where the item is intact, the operation stale where it is not.
    ///
    /// Files the store's own runs wrote since the last call are not "underneath" it — the run
    /// already carried every plan through those edits — and are skipped.
    pub fn reresolve_files(
        &mut self,
        files: &[String],
        resolver: &mut dyn ItemResolver,
    ) -> Result<()> {
        live::reresolve(self, files, resolver)
    }

    /// The stale operations of a held plan, in plan order. Derived, never written to the plan; a
    /// plan that is not held has none.
    pub fn stale_ops(&self, key: &PlanKey) -> Vec<OpStaleness> {
        live::stale_ops(self, key)
    }

    /// Write every plan dirty for longer than the debounce; the keys written.
    pub fn flush_dirty(&mut self) -> Result<Vec<PlanKey>> {
        let debounce = self.policy.debounce;
        let due: Vec<PlanKey> = self
            .dirty_since
            .iter()
            .filter(|(_, since)| since.elapsed() >= debounce)
            .map(|(key, _)| key.clone())
            .collect();
        self.write_all(due)
    }

    /// Write one plan now if it is dirty — what a run does after each operation it commits.
    pub fn flush(&mut self, key: &PlanKey) -> Result<()> {
        self.write_back(key)
    }

    /// Write every dirty plan now — end of run, unload, shutdown; the keys written.
    pub fn flush_all(&mut self) -> Result<Vec<PlanKey>> {
        self.write_all(self.dirty_since.keys().cloned().collect())
    }

    /// Write each of `keys`, trying every one and returning the first failure.
    fn write_all(&mut self, keys: Vec<PlanKey>) -> Result<Vec<PlanKey>> {
        let mut written = Vec::new();
        let mut first_failure = None;
        for key in keys {
            match self.write_back(&key) {
                Ok(()) => written.push(key),
                Err(failure) => {
                    first_failure.get_or_insert(failure);
                }
            }
        }
        first_failure.map_or(Ok(written), Err)
    }

    /// Read a plan's file into a store entry: ids assigned, a repeated id refused.
    fn read(&self, key: &PlanKey) -> Result<LoadedPlan> {
        let bytes = std::fs::read(self.path_of(key))?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            RestructureError::MalformedPlan(format!("{key}: the plan is not UTF-8 text"))
        })?;
        let mut plan = Plan::parse(text)?;
        if let Some(id) = plan.repeated_op_id() {
            return Err(RestructureError::MalformedPlan(format!(
                "{key}: two operations share the id `{id}`"
            )));
        }
        let dirty = plan.assign_missing_op_ids();
        Ok(LoadedPlan {
            key: key.clone(),
            plan,
            dirty,
            on_disk: hash_of(&bytes),
        })
    }

    fn held(&self, key: &PlanKey) -> Result<&LoadedPlan> {
        self.plans.get(key).ok_or_else(|| {
            RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
        })
    }

    fn mark_dirty(&mut self, key: &PlanKey) {
        if let Some(held) = self.plans.get_mut(key) {
            held.dirty = true;
            self.dirty_since
                .entry(key.clone())
                .or_insert_with(Instant::now);
        }
    }

    /// Write `key`'s plan back and drop it, unless the file moved on without the store.
    fn release(&mut self, key: &PlanKey) -> Result<()> {
        match self.write_back(key) {
            Ok(()) | Err(RestructureError::PlanChangedOnDisk { .. }) => {}
            Err(failure) => return Err(failure),
        }
        self.plans.remove(key);
        self.dirty_since.remove(key);
        self.live.forget(key);
        Ok(())
    }

    /// Write `key`'s plan if it is dirty: a temporary file beside it, then a rename, so a reader
    /// sees the old plan or the new one and never half of either.
    ///
    /// Refused, writing nothing, when the file's content is no longer what the store read or wrote
    /// last. The check and the rename are two steps, not one: an edit landing between them would
    /// still be replaced.
    fn write_back(&mut self, key: &PlanKey) -> Result<()> {
        let held = self.held(key)?;
        if !held.dirty {
            return Ok(());
        }

        let path = self.path_of(key);
        let unchanged = std::fs::read(&path)
            .map(|bytes| hash_of(&bytes) == held.on_disk)
            .unwrap_or(false);
        if !unchanged {
            return Err(RestructureError::PlanChangedOnDisk {
                plan: key.to_string(),
            });
        }

        let text = held.plan.to_jsonl();
        let mut staged = path.clone().into_os_string();
        staged.push(format!(".{}.tmp", std::process::id()));
        let staged = PathBuf::from(staged);
        std::fs::write(&staged, &text)?;
        if let Err(failure) = std::fs::rename(&staged, &path) {
            let _ = std::fs::remove_file(&staged);
            return Err(failure.into());
        }

        if let Some(held) = self.plans.get_mut(key) {
            held.dirty = false;
            held.on_disk = hash_of(text.as_bytes());
        }
        self.dirty_since.remove(key);
        Ok(())
    }
}

/// A digest of the anchors of every operation after position `applied`: what a run records after
/// each operation it commits, and what a resume checks the plan it reads against.
///
/// Anchors and ids only. A person may reword a pending operation's `name` between runs and the
/// digest does not move; anything that could change *where* an operation lands does move it.
pub fn pending_digest(plan: &Plan, applied: usize) -> String {
    let pending: Vec<_> = plan
        .ops
        .iter()
        .skip(applied + 1)
        .map(|op| (&op.id, &op.anchor, &op.also))
        .collect();
    hash_of(&serde_json::to_vec(&pending).expect("anchors serialise"))
}

fn summary_of(held: &LoadedPlan) -> PlanSummary {
    PlanSummary {
        key: held.key.clone(),
        ops: held.plan.ops.len(),
        dirty: held.dirty,
    }
}

/// `sha256:<hex>` of a file's bytes.
fn hash_of(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// `path` with `.` and `..` folded away lexically.
fn normalised(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded
}

mod live;
mod refresh;
pub use refresh::rebase_plan_file;

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

    /// A resolver that no longer finds any item — the edit moved it out of its file.
    struct AResolverThatFindsNothing;

    impl ItemResolver for AResolverThatFindsNothing {
        fn resolve_item(&mut self, _file: &str, item: &ItemPath) -> Result<ResolvedItem> {
            Err(RestructureError::MalformedPlan(format!(
                "`{item}` is not in the file"
            )))
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

    const A_RENAME: &str = r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#;

    #[test]
    fn unloading_writes_the_plan_back_and_drops_it() {
        // Given a held plan made dirty by the ids its load gave
        let (root, mut store, _key) = a_store_holding(&[A_RENAME]);

        // When it is unloaded
        store.unload(&[PathBuf::from("plan.jsonl")]).unwrap();

        // Then the file carries the id and the store holds nothing
        let written =
            Plan::parse(&std::fs::read_to_string(root.path().join("plan.jsonl")).unwrap()).unwrap();
        assert_eq!(
            (written.ops[0].id.is_some(), store.list()),
            (true, Vec::new())
        );
    }

    #[test]
    fn unloading_a_plan_whose_file_changed_on_disk_drops_it_and_leaves_the_file() {
        // Given a held, dirty plan whose file was then edited by hand
        let (root, mut store, _key) = a_store_holding(&[A_RENAME]);
        std::fs::write(root.path().join("plan.jsonl"), format!("{HEADER}\n")).unwrap();

        // When it is unloaded
        store.unload(&[PathBuf::from("plan.jsonl")]).unwrap();

        // Then the hand edit stands and the store no longer holds the plan
        assert_eq!(
            (
                std::fs::read_to_string(root.path().join("plan.jsonl")).unwrap(),
                store.list()
            ),
            (format!("{HEADER}\n"), Vec::new())
        );
    }

    #[test]
    fn unloading_a_plan_that_is_not_loaded_is_refused_and_drops_nothing() {
        // Given a store holding one plan
        let (_root, mut store, _key) = a_store_holding(&[A_RENAME]);

        // When it is asked to unload that plan and another it never held
        let unloaded = store.unload(&[PathBuf::from("plan.jsonl"), PathBuf::from("other.jsonl")]);

        // Then it is refused naming the other, and the held plan is still held
        assert_eq!(
            (
                unloaded.map_err(|error| error.to_string()),
                store.list().len()
            ),
            (
                Err("plan is malformed: other.jsonl is not loaded — load it first".to_string()),
                1
            )
        );
    }

    #[test]
    fn a_load_that_refuses_one_plan_loads_none_of_them() {
        // Given a store and two plan files, the second malformed
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("good.jsonl"),
            format!("{HEADER}\n{A_RENAME}\n"),
        )
        .unwrap();
        std::fs::write(root.path().join("bad.jsonl"), "not a plan\n").unwrap();
        let mut store = PlanStore::new(
            root.path(),
            FlushPolicy {
                debounce: Duration::from_secs(3600),
            },
        );

        // When both are loaded
        let loaded = store.load(&[PathBuf::from("good.jsonl"), PathBuf::from("bad.jsonl")]);

        // Then the load is refused and the good plan is not held either
        assert_eq!((loaded.is_err(), store.list()), (true, Vec::new()));
    }

    #[test]
    fn a_plan_is_keyed_the_same_whether_named_relatively_or_absolutely() {
        // Given a store rooted at a directory
        let (root, store, key) = a_store_holding(&[]);

        // When the plan is named three ways
        let named_absolutely = store.key_for(&root.path().join("plan.jsonl")).unwrap();
        let named_through_dots = store.key_for(Path::new("./sub/../plan.jsonl")).unwrap();

        // Then all are one key
        assert_eq!((named_absolutely, named_through_dots), (key.clone(), key));
    }

    #[test]
    fn a_pending_item_anchor_the_edit_left_unresolvable_keeps_its_anchor_as_written() {
        // Given a pending item anchor in the file the first op will edit
        let (_root, mut store, key) = a_store_holding(&[
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            r#"{"op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:before","hint":{"line":20,"col":5}},"name":"g"}"#,
        ]);
        let first = the_id_of(&store, &key, 0);
        let as_written = the_anchor_of(&store, &key, 1);

        // When the first op's edit touched the file, and the item no longer resolves in it
        let refreshed = store.refresh_after_op(
            &key,
            &first,
            &three_lines_inserted_at_the_top_of("src/a.rs"),
            &mut AResolverThatFindsNothing,
        );

        // Then the refresh succeeds and the anchor is exactly what the plan said
        assert_eq!(
            (refreshed.is_ok(), the_anchor_of(&store, &key, 1)),
            (true, as_written)
        );
    }

    #[test]
    fn an_operation_is_found_by_its_id_whatever_its_place_in_the_plan() {
        // Given a held plan of two operations
        let (_root, store, key) = a_store_holding(&[
            A_RENAME,
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"C"},"name":"D"}"#,
        ]);
        let second = the_id_of(&store, &key, 1);

        // When the second is looked up by its id, and an id the plan never gave is looked up
        let found = store.op(&key, &second).map(|op| op.name.clone());
        let missing = store.op(&key, &OpId("op-99".to_string()));

        // Then the second operation is returned and the unknown id finds nothing
        assert_eq!((found, missing), (Some(Some("D".to_string())), None));
    }
}

#[cfg(test)]
mod live_plans_tests {
    use super::*;
    use crate::edit::{Position, TextEdit};
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
