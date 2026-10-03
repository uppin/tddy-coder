//! Folding one operation of a held plan into every other held plan.
//!
//! Judged line by line against the edit the operation produced, in the coordinates of the text the
//! edit was made against: what it overlaps is stale, what it merely moves is moved.

use std::path::Path;

use super::super::refresh::resolved_or_left_as_written;
use super::super::{LoadedPlan, PlanKey, PlanStore, StaleReason};
use super::{commit, followed_hints, Refreshed};
use crate::edit::{FileEdit, Position, TextEdit, WorkspaceEdit};
use crate::item_anchor::{absolute_range, ItemResolver, ResolvedItem};
use crate::ledger::PositionLedger;
use crate::plan::{Anchor, ItemPath, OpId, RefactorOp};
use crate::{Fingerprint, RestructureError, Result};

/// Whether `edit` changes any of lines `first..=last` of the text it was made against.
///
/// A whole-line insertion between two lines changes none of them — and so moves an anchored range
/// without editing it — unless it lands between two lines the range covers. Any other edit changes
/// the lines from its start line to its end line, the end line excluded when the edit stops at its
/// first column.
fn overlaps(edit: &TextEdit, first: u32, last: u32) -> bool {
    let (start, end) = (edit.range.start, edit.range.end);
    let whole_lines = edit.new_text.is_empty() || edit.new_text.ends_with('\n');
    if start == end && start.col == 1 && whole_lines {
        return !edit.new_text.is_empty() && start.line > first && start.line <= last;
    }
    let last_edited = if end.col == 1 && end.line > start.line {
        end.line - 1
    } else {
        end.line
    };
    start.line <= last && last_edited >= first
}

/// One operation of another plan, as far as the plans that did not run it are concerned.
struct Fold<'a> {
    edit: &'a WorkspaceEdit,
    ledger: PositionLedger,
}

impl Fold<'_> {
    /// Where a file the edit may have moved is now.
    fn followed(&self, file: &str) -> String {
        self.ledger
            .current_path(Path::new(file))
            .display()
            .to_string()
    }

    /// The text edits made to `file`, whether the edit names it as it was or as it is.
    fn edits_to(&self, file: &str, now: &str) -> Vec<&TextEdit> {
        self.edit
            .changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Change { path, edits } if path == file || path == now => Some(edits),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn edited_inside(&self, file: &str, now: &str, first: u32, last: u32) -> bool {
        self.edits_to(file, now)
            .iter()
            .any(|edit| overlaps(edit, first, last))
    }

    /// `at` once the edit has been made, or `None` when the edit removed the text it was in.
    fn translate(&self, file: &str, at: Position) -> Result<Option<Position>> {
        match self.ledger.translate(Path::new(file), at) {
            Ok(moved) => Ok(Some(moved)),
            Err(RestructureError::AnchorInvalidated { .. }) => Ok(None),
            Err(failure) => Err(failure),
        }
    }
}

/// Fold one operation, which produced `edit`, into every held plan but `from`.
pub(in crate::plan_store) fn fold(
    store: &mut PlanStore,
    from: &PlanKey,
    op: &OpId,
    edit: &WorkspaceEdit,
    resolver: &mut dyn ItemResolver,
) -> Result<()> {
    let touched = crate::apply::touched_paths(edit);
    store.live.own_writes.extend(touched.iter().cloned());
    let mut ledger = PositionLedger::new();
    ledger.record(edit);
    let fold = Fold { edit, ledger };

    // Every plan is worked out before any is changed, so a refusal part-way leaves them all as they
    // were.
    let reason = StaleReason::EditedBy {
        plan: from.clone(),
        op: op.clone(),
    };
    let mut folded = Vec::new();
    for (key, held) in store.plans.iter().filter(|(key, _)| *key != from) {
        let refreshed = fold_plan(store, held, (&fold, &reason), &touched, resolver)?;
        folded.push((key.clone(), refreshed));
    }
    for (key, refreshed) in folded {
        commit(store, &key, refreshed);
    }
    Ok(())
}

/// `held` after the fold: `through` is the edit and what an operation it overlaps is stale for.
fn fold_plan(
    store: &PlanStore,
    held: &LoadedPlan,
    through: (&Fold, &StaleReason),
    touched: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<Refreshed> {
    let (fold, reason) = through;
    let mut ops = held.plan.ops.clone();
    let mut verdicts = Vec::new();
    // TODO(live-plans): every operation is folded, including ones the plan already ran, because the
    // store holds no journal. An operation that already ran can be reported stale for an edit that
    // overlaps where it used to anchor; a run only refuses the ones at or after its start.
    for op in &mut ops {
        let Some(id) = op.id.clone() else { continue };
        // A stale operation is left exactly as it was: following the tree would be re-targeting it.
        if store.live.reason_for(&held.key, &id).is_some() {
            continue;
        }
        match fold_op(fold, op, resolver)? {
            Some(folded) => *op = folded,
            None => verdicts.push((id, Some(reason.clone()))),
        }
    }
    let files = followed_hints(&held.plan.files, &fold.ledger, &store.root, touched)?;
    Ok(Refreshed {
        ops,
        files,
        verdicts,
    })
}

/// `op` as the edit left the tree, or `None` when the edit overlapped something it names.
fn fold_op(
    fold: &Fold,
    op: &RefactorOp,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<RefactorOp>> {
    let Some(anchor) = fold_anchor(fold, &op.anchor, resolver)? else {
        return Ok(None);
    };
    let mut also = Vec::with_capacity(op.also.len());
    for member in &op.also {
        match fold_anchor(fold, member, resolver)? {
            Some(folded) => also.push(folded),
            None => return Ok(None),
        }
    }
    let mut folded = op.with_anchor(anchor);
    folded.also = also;
    Ok(Some(folded))
}

fn fold_anchor(
    fold: &Fold,
    anchor: &Anchor,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    match anchor {
        Anchor::Symbol { file, path } => Ok(Some(Anchor::Symbol {
            file: fold.followed(file),
            path: path.clone(),
        })),
        Anchor::Range { file, start, end } => fold_range(fold, file, *start, *end),
        Anchor::Item { .. } => fold_item(fold, anchor, resolver),
        Anchor::Items {
            file,
            items,
            fingerprints,
        } => fold_items(fold, file, items, fingerprints, resolver),
    }
}

fn fold_range(fold: &Fold, file: &str, start: Position, end: Position) -> Result<Option<Anchor>> {
    let now = fold.followed(file);
    if fold.edited_inside(file, &now, start.line, end.line) {
        return Ok(None);
    }
    let (Some(start), Some(end)) = (fold.translate(file, start)?, fold.translate(file, end)?)
    else {
        return Ok(None);
    };
    Ok(Some(Anchor::Range {
        file: now,
        start,
        end,
    }))
}

/// An item anchor after the edit: it follows its item when the item is untouched or the edit stayed
/// outside the range it names, and is `None` when the edit reached into that range.
///
/// Judged against the tree the edit left: the item resolves there, and a fingerprint that moved
/// says the edit touched the item. Where in the item it touched is then read from the anchor's
/// `hint`, the only record of where the anchored range sat before.
fn fold_item(
    fold: &Fold,
    anchor: &Anchor,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    let Anchor::Item {
        item,
        file,
        start,
        end,
        fingerprint,
        hint,
    } = anchor
    else {
        return Ok(Some(anchor.clone()));
    };
    let now = fold.followed(file);
    let mut followed = Anchor::Item {
        item: item.clone(),
        file: now.clone(),
        start: *start,
        end: *end,
        fingerprint: fingerprint.clone(),
        hint: *hint,
    };
    if fold.edits_to(file, &now).is_empty() {
        return Ok(Some(followed));
    }
    let Some(found) = resolved_or_left_as_written(resolver.resolve_item(&now, item))? else {
        return Ok(None);
    };
    if found.fingerprint != *fingerprint {
        return follow_changed_item(fold, file, &found, &followed);
    }
    if let (Anchor::Item { hint, .. }, Ok(range)) =
        (&mut followed, absolute_range(&found, *start, *end))
    {
        *hint = Some(range.start);
    }
    Ok(Some(followed))
}

/// The anchor of an item the edit changed, followed through the edit when the edit stayed outside
/// the range the anchor names.
fn follow_changed_item(
    fold: &Fold,
    written_in: &str,
    found: &ResolvedItem,
    anchor: &Anchor,
) -> Result<Option<Anchor>> {
    let Anchor::Item {
        item,
        file,
        start,
        end,
        hint,
        ..
    } = anchor
    else {
        return Ok(Some(anchor.clone()));
    };
    let Some(hint) = *hint else {
        return Ok(None);
    };
    let relative = start.zip(*end);
    let lines = relative.map_or(0, |(start, end)| end.line.saturating_sub(start.line));
    if fold.edited_inside(written_in, file, hint.line, hint.line + lines) {
        return Ok(None);
    }
    let (start, end, hint) = match relative {
        None => (None, None, found.name),
        Some(relative) => {
            let Some((start, end)) = rebased_range(fold, written_in, found, hint, relative)? else {
                return Ok(None);
            };
            let Ok(range) = absolute_range(found, Some(start), Some(end)) else {
                return Ok(None);
            };
            (Some(start), Some(end), range.start)
        }
    };
    Ok(Some(Anchor::Item {
        item: item.clone(),
        file: file.clone(),
        start,
        end,
        fingerprint: found.fingerprint.clone(),
        hint: Some(hint),
    }))
}

/// The range an item anchor names, relative to its item as the edit left it: the absolute range it
/// had (`hint` is where it started) carried through the edit, then measured from the item's new
/// first line.
fn rebased_range(
    fold: &Fold,
    written_in: &str,
    found: &ResolvedItem,
    hint: Position,
    relative: (Position, Position),
) -> Result<Option<(Position, Position)>> {
    let (start, end) = relative;
    let last = Position {
        line: hint.line + end.line.saturating_sub(start.line),
        col: end.col,
    };
    let (Some(new_start), Some(new_end)) = (
        fold.translate(written_in, hint)?,
        fold.translate(written_in, last)?,
    ) else {
        return Ok(None);
    };
    let first = found.range.start.line;
    if new_start.line < first || new_end.line < first {
        return Ok(None);
    }
    Ok(Some((
        Position {
            line: new_start.line - first + 1,
            col: start.col,
        },
        Position {
            line: new_end.line - first + 1,
            col: end.col,
        },
    )))
}

fn fold_items(
    fold: &Fold,
    file: &str,
    items: &[ItemPath],
    fingerprints: &[Fingerprint],
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    let now = fold.followed(file);
    let followed = Anchor::Items {
        file: now.clone(),
        items: items.to_vec(),
        fingerprints: fingerprints.to_vec(),
    };
    if fold.edits_to(file, &now).is_empty() {
        return Ok(Some(followed));
    }
    for (item, fingerprint) in items.iter().zip(fingerprints) {
        match resolved_or_left_as_written(resolver.resolve_item(&now, item))? {
            Some(found) if found.fingerprint == *fingerprint => {}
            _ => return Ok(None),
        }
    }
    Ok(Some(followed))
}
