//! `repoint_call`: re-pointing the part of a call that is in front of its argument list.
//!
//! The **single form** replaces the callee of exactly one call (`self.slot(x)` becomes
//! `self.peer.slot(x)`), the arguments byte for byte. The **bulk form**, anchored on a method with
//! no range, inserts hops after the receiver of every call of that method the server knows
//! (`x.m(..)` becomes `x.agent_roster().m(..)`). Both are text edits authored here; the only
//! server-informed part is the reference set of the bulk form. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-repoint-call.md` for the rules.

mod receivers;
mod single;
mod sites;

use super::{failure, RustBackend};
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::item_anchor::unlowered_item_anchor;
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `repoint_call`, from the plan and the text alone.
///
/// Only a range anchor is examined: the range must be exactly one call, the new callee must differ
/// from the current one, and the old callee must hold no call. An item anchor is not lowered at a
/// plain check, so it says nothing here — the runner's own "anchors by item" finding covers it —
/// and the zero-width range the bulk form lowers to is not a call to examine.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let Anchor::Range { file, start, end } = &op.anchor else {
        return Ok(Vec::new());
    };
    if start == end {
        return Ok(Vec::new());
    }
    let Some(callee) = op.callee.as_deref() else {
        return Ok(Vec::new());
    };
    let text = workspace.read(file)?;
    let range = Range {
        start: *start,
        end: *end,
    };
    match single::rewrite_callee(&text, range, callee) {
        Ok(_) => Ok(Vec::new()),
        Err(refusal) => Ok(vec![refusal.to_string()]),
    }
}

impl RustBackend {
    /// Resolve a `repoint_call` into the edits to the files that hold the calls.
    ///
    /// A range with width is the single form; the zero-width range an item anchor with no range
    /// lowers to, at the method's name, is the bulk form.
    pub(super) fn repoint_call(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let Anchor::Range {
            file, start, end, ..
        } = &op.anchor
        else {
            return Err(unlowered_item_anchor(&op.anchor, "RepointCall"));
        };
        let callee = op
            .callee
            .as_deref()
            .ok_or_else(|| failure("`repoint_call` needs `callee`"))?;
        if start == end {
            return sites::repoint_receivers(self, op, workspace);
        }
        let text = workspace.read(file)?;
        let range = Range {
            start: *start,
            end: *end,
        };
        let edits = single::rewrite_callee(&text, range, callee)?;
        Ok(Resolution::of(WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: file.clone(),
                edits,
            }],
        }))
    }
}
