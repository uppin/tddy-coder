//! Keeping every held plan current as the tree moves underneath it.
//!
//! Two things move a held plan's anchors besides its own operations: an operation of **another**
//! held plan ([`fold`]), and a change nobody here made — a hand edit, a pull ([`reresolve`]). Both
//! rewrite what is safe to rewrite and mark the rest **stale**, and neither re-targets a stale
//! operation: an edit that overlaps an anchored range could have removed what the range names, so
//! the author re-anchors it.
//!
//! Staleness is derived and held in memory beside the plans, never serialised into them: the plan
//! on disk stays a plan, and what was stale is worked out again from the tree.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::refresh::resolved_or_left_as_written;
use super::{LoadedPlan, OpStaleness, PlanKey, PlanStore, StaleReason};
use crate::item_anchor::{absolute_range, ItemResolver};
use crate::ledger::PositionLedger;
use crate::plan::{hint_of, Anchor, FileHint, OpId, Plan, RefactorOp};
use crate::Result;

mod fold;
pub(super) use fold::fold;

/// What a store knows about its plans beyond the plans themselves.
#[derive(Debug, Default)]
pub(super) struct Liveness {
    stale: BTreeMap<PlanKey, BTreeMap<OpId, StaleReason>>,
    /// The files the store's own runs wrote since the tree was last compared: the next
    /// [`reresolve`] discounts them, because the run that wrote them already carried every held
    /// plan through the edit.
    own_writes: BTreeSet<String>,
}

impl Liveness {
    pub(super) fn forget(&mut self, key: &PlanKey) {
        self.stale.remove(key);
    }

    /// Record why `op` is stale. The first reason stands: a later one describes the same operation
    /// the same way worse, and the author re-anchors it either way.
    fn mark(&mut self, key: &PlanKey, op: OpId, reason: StaleReason) {
        self.stale
            .entry(key.clone())
            .or_default()
            .entry(op)
            .or_insert(reason);
    }

    /// Forget that `op`'s item changed or went missing, because the tree says it is intact again.
    /// An operation another plan edited stays stale: no re-resolution undoes that.
    fn clear_item_staleness(&mut self, key: &PlanKey, op: &OpId) {
        if let Some(ops) = self.stale.get_mut(key) {
            if matches!(
                ops.get(op),
                Some(StaleReason::ItemChanged | StaleReason::ItemNotFound { .. })
            ) {
                ops.remove(op);
            }
        }
    }

    fn reason_for(&self, key: &PlanKey, op: &OpId) -> Option<&StaleReason> {
        self.stale.get(key)?.get(op)
    }
}

/// The stale operations of a held plan, in plan order.
pub(super) fn stale_ops(store: &PlanStore, key: &PlanKey) -> Vec<OpStaleness> {
    let Some(held) = store.plans.get(key) else {
        return Vec::new();
    };
    held.plan
        .ops
        .iter()
        .filter_map(|op| {
            let id = op.id.as_ref()?;
            let reason = store.live.reason_for(key, id)?;
            Some(OpStaleness {
                op: id.clone(),
                reason: reason.clone(),
            })
        })
        .collect()
}

/// A plan as a pass over the tree left it: the changed operations and file hints, and the
/// operations that pass found stale.
struct Refreshed {
    ops: Vec<RefactorOp>,
    files: BTreeMap<String, FileHint>,
    /// `Some` for an operation judged against the tree: why it is stale, or `None` when it is not.
    verdicts: Vec<(OpId, Option<StaleReason>)>,
}

/// Write a pass's anchors and hints into the plan it was made from, marking the plan dirty when
/// they differ from what it held.
fn commit(store: &mut PlanStore, key: &PlanKey, refreshed: Refreshed) {
    let Some(held) = store.plans.get_mut(key) else {
        return;
    };
    let changed = held.plan.ops != refreshed.ops || held.plan.files != refreshed.files;
    if changed {
        held.plan.ops = refreshed.ops;
        held.plan.files = refreshed.files;
        store.mark_dirty(key);
    }
    for (op, verdict) in refreshed.verdicts {
        store.live.clear_item_staleness(key, &op);
        if let Some(reason) = verdict {
            store.live.mark(key, op, reason);
        }
    }
}

/// The per-file hints of a plan after the edit: each moved with its file, and rewritten for every
/// file the edit touched. A file that is not on disk keeps the hint it had — a hint is advice, and
/// there is nothing to take it from.
fn followed_hints(
    hints: &BTreeMap<String, FileHint>,
    ledger: &PositionLedger,
    root: &Path,
    touched: &[String],
) -> Result<BTreeMap<String, FileHint>> {
    let mut followed = BTreeMap::new();
    for (path, hint) in hints {
        let now = ledger.current_path(Path::new(path)).display().to_string();
        let affected = now != *path || touched.contains(path) || touched.contains(&now);
        let fresh = if affected {
            rewritten_hint(root, &now)?
        } else {
            None
        };
        followed.insert(now, fresh.unwrap_or_else(|| hint.clone()));
    }
    Ok(followed)
}

fn rewritten_hint(root: &Path, file: &str) -> Result<Option<FileHint>> {
    let path = root.join(file);
    if !path.is_file() {
        return Ok(None);
    }
    hint_of(&path).map(Some)
}

/// Re-resolve the held item anchors in `files`, which changed underneath the store.
pub(super) fn reresolve(
    store: &mut PlanStore,
    files: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<()> {
    let own = std::mem::take(&mut store.live.own_writes);
    let changed: Vec<String> = files
        .iter()
        .filter(|file| !own.contains(*file))
        .cloned()
        .collect();
    if changed.is_empty() {
        return Ok(());
    }

    let mut judged = Vec::new();
    for (key, held) in &store.plans {
        judged.push((key.clone(), rejudge_plan(store, held, &changed, resolver)?));
    }
    for (key, refreshed) in judged {
        commit(store, &key, refreshed);
    }
    Ok(())
}

fn rejudge_plan(
    store: &PlanStore,
    held: &LoadedPlan,
    changed: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<Refreshed> {
    let mut ops = held.plan.ops.clone();
    let mut verdicts = Vec::new();
    for op in &mut ops {
        let Some(id) = op.id.clone() else { continue };
        let edited_by_another = matches!(
            store.live.reason_for(&held.key, &id),
            Some(StaleReason::EditedBy { .. })
        );
        if edited_by_another
            || !op
                .anchors()
                .any(|anchor| changed.iter().any(|file| file == anchor.file()))
        {
            continue;
        }
        let (rejudged, verdict) = rejudge_op(op, changed, resolver)?;
        *op = rejudged;
        verdicts.push((id, verdict));
    }
    let files = followed_hints(
        &held.plan.files,
        &PositionLedger::new(),
        &store.root,
        changed,
    )?;
    Ok(Refreshed {
        ops,
        files,
        verdicts,
    })
}

/// `op` with the hints of its intact anchors rewritten, and why it is stale if any anchor is.
fn rejudge_op(
    op: &RefactorOp,
    changed: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<(RefactorOp, Option<StaleReason>)> {
    let mut stale = None;
    let (anchor, reason) = rejudge_anchor(&op.anchor, changed, resolver)?;
    stale = stale.or(reason);
    let mut rejudged = op.with_anchor(anchor);
    for (member, held) in rejudged.also.iter_mut().zip(&op.also) {
        let (anchor, reason) = rejudge_anchor(held, changed, resolver)?;
        *member = anchor;
        stale = stale.or(reason);
    }
    Ok((rejudged, stale))
}

fn rejudge_anchor(
    anchor: &Anchor,
    changed: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<(Anchor, Option<StaleReason>)> {
    match anchor {
        Anchor::Item {
            item,
            file,
            start,
            end,
            fingerprint,
            ..
        } if changed.contains(file) => {
            let Some(found) = resolved_or_left_as_written(resolver.resolve_item(file, item))?
            else {
                return Ok((anchor.clone(), Some(not_found_in(file))));
            };
            if found.fingerprint != *fingerprint {
                return Ok((anchor.clone(), Some(StaleReason::ItemChanged)));
            }
            let mut hinted = anchor.clone();
            if let (Anchor::Item { hint, .. }, Ok(range)) =
                (&mut hinted, absolute_range(&found, *start, *end))
            {
                *hint = Some(range.start);
            }
            Ok((hinted, None))
        }
        Anchor::Items {
            file,
            items,
            fingerprints,
        } if changed.contains(file) => {
            for (item, fingerprint) in items.iter().zip(fingerprints) {
                let Some(found) = resolved_or_left_as_written(resolver.resolve_item(file, item))?
                else {
                    return Ok((anchor.clone(), Some(not_found_in(file))));
                };
                if found.fingerprint != *fingerprint {
                    return Ok((anchor.clone(), Some(StaleReason::ItemChanged)));
                }
            }
            Ok((anchor.clone(), None))
        }
        _ => Ok((anchor.clone(), None)),
    }
}

fn not_found_in(file: &str) -> StaleReason {
    StaleReason::ItemNotFound {
        file: file.to_string(),
    }
}

/// Every file `plan` names: where its item anchors look, and what its header carries hints for.
/// What a rebase of a plan nobody holds re-resolves.
pub(super) fn files_named_by(plan: &Plan) -> Vec<String> {
    let anchored = plan
        .ops
        .iter()
        .flat_map(|op| op.anchors())
        .filter(|anchor| matches!(anchor, Anchor::Item { .. } | Anchor::Items { .. }))
        .map(|anchor| anchor.file().to_string());
    anchored
        .chain(plan.files.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
