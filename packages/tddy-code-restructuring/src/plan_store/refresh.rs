use super::OpStaleness;

use crate::{RefactorOp, RestructureError};

use crate::item_anchor::absolute_range;

use std::path::Path;

use crate::plan::Anchor;

use crate::edit::FileEdit;

use crate::ledger::PositionLedger;

use crate::Result;

use crate::item_anchor::ItemResolver;

use crate::edit::WorkspaceEdit;

/// Bring each of `pending` up to the tree `edit` left behind; whether any of them changed.
///
/// Also what a dry run uses on a copy of the plan, where nothing is written and the edit is only
/// rehearsed.
pub(crate) fn refresh_pending(
    pending: &mut [RefactorOp],
    edit: &WorkspaceEdit,
    resolver: &mut dyn ItemResolver,
) -> Result<bool> {
    let mut ledger = PositionLedger::new();
    ledger.record(edit);
    let edited: Vec<&str> = edit
        .changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change { path, .. } => Some(path.as_str()),
            FileEdit::Create { .. } | FileEdit::Rename { .. } => None,
        })
        .collect();

    let mut changed = false;
    for op in pending {
        let anchor = refreshed(&op.anchor, &ledger, &edited, resolver)?;
        let also = op
            .also
            .iter()
            .map(|member| refreshed(member, &ledger, &edited, resolver))
            .collect::<Result<Vec<_>>>()?;
        if anchor != op.anchor || also != op.also {
            op.anchor = anchor;
            op.also = also;
            changed = true;
        }
    }
    Ok(changed)
}

/// One anchor as the tree the edit left behind reads it.
fn refreshed(
    anchor: &Anchor,
    ledger: &PositionLedger,
    edited: &[&str],
    resolver: &mut dyn ItemResolver,
) -> Result<Anchor> {
    let followed = |file: &str| ledger.current_path(Path::new(file)).display().to_string();
    match anchor {
        Anchor::Symbol { .. } | Anchor::Range { .. } => ledger.translate_anchor(anchor),
        Anchor::Item {
            item,
            file,
            start,
            end,
            fingerprint,
            hint,
        } => {
            let file = followed(file);
            let mut anchor = Anchor::Item {
                item: item.clone(),
                file: file.clone(),
                start: *start,
                end: *end,
                fingerprint: fingerprint.clone(),
                hint: *hint,
            };
            if edited.contains(&file.as_str()) {
                let Some(found) = resolved_or_left_as_written(resolver.resolve_item(&file, item))?
                else {
                    return Ok(anchor);
                };
                let Some(range) =
                    resolved_or_left_as_written(absolute_range(&found, *start, *end))?
                else {
                    return Ok(anchor);
                };
                if let Anchor::Item {
                    fingerprint, hint, ..
                } = &mut anchor
                {
                    *fingerprint = found.fingerprint;
                    *hint = Some(range.start);
                }
            }
            Ok(anchor)
        }
        Anchor::Items {
            file,
            items,
            fingerprints,
        } => {
            let file = followed(file);
            let mut refreshed_fingerprints = Vec::with_capacity(items.len());
            if edited.contains(&file.as_str()) {
                for item in items {
                    match resolved_or_left_as_written(resolver.resolve_item(&file, item))? {
                        Some(found) => refreshed_fingerprints.push(found.fingerprint),
                        None => {
                            refreshed_fingerprints.clear();
                            break;
                        }
                    }
                }
            }
            Ok(Anchor::Items {
                file,
                items: items.clone(),
                fingerprints: if refreshed_fingerprints.len() == items.len() {
                    refreshed_fingerprints
                } else {
                    fingerprints.clone()
                },
            })
        }
    }
}

/// What an answer says, or `None` when it is the refusal of an item the edit left no longer
/// resolvable. Every other failure — a server that did not answer, a caller that stopped waiting —
/// is not about the item, and is returned.
fn resolved_or_left_as_written<T>(answer: Result<T>) -> Result<Option<T>> {
    match answer {
        Ok(found) => Ok(Some(found)),
        Err(RestructureError::MalformedPlan(_)) => Ok(None),
        Err(failure) => Err(failure),
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
