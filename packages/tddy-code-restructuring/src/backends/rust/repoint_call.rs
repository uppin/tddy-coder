//! `repoint_call`: re-pointing the part of a call that is in front of its argument list.
//!
//! The **single form** replaces the callee of exactly one call (`self.slot(x)` becomes
//! `self.peer.slot(x)`), the arguments byte for byte. The **bulk form**, anchored on a method with
//! no range, inserts hops after the receiver of every call of that method the server knows
//! (`x.m(..)` becomes `x.agent_roster().m(..)`). Both are text edits authored here; the only
//! server-informed part is the reference set of the bulk form. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-repoint-call.md` for the rules.
//!
//! TODO(repoint-call): the operation is published and not implemented. Until it is, `resolve`
//! refuses naming the node, so nothing believes a call was re-pointed. `check` finds nothing to
//! report from the text alone, on purpose: a static finding stops `check --deep` from rehearsing
//! (`check_plan` skips `resolve` for an operation with one), and the rehearsal is where the refusal
//! is reported.

mod receivers;
mod single;
mod sites;

use super::RustBackend;
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::item_anchor::unlowered_item_anchor;
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::{RestructureError, Result};

/// What a static check finds wrong with a `repoint_call`, from the plan and the text alone.
///
/// TODO(repoint-call): implement the single form's text rules for a range anchor (the range is one
/// call, the new callee differs from the current one, the old callee holds no call). Nothing is
/// reported until then, so `check --deep` still reaches the refusal `resolve` raises.
pub(super) fn findings(_op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Vec<String>> {
    Ok(Vec::new())
}

/// The refusal an unimplemented step raises: the backend does not support the operation yet.
fn unfinished() -> RestructureError {
    RestructureError::UnsupportedOp {
        backend: "Rust (node `repoint-call` has not implemented it yet, TODO(repoint-call))"
            .to_string(),
        op: "RepointCall".to_string(),
    }
}

impl RustBackend {
    /// Resolve a `repoint_call` into the edits to the files that hold the calls.
    ///
    /// A range with width is the single form; the zero-width range an item anchor with no range
    /// lowers to, at the method's name, is the bulk form.
    ///
    /// TODO(repoint-call): implement `single::rewrite_callee` and `sites::repoint_receivers`.
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
        let callee = op.callee.as_deref().ok_or_else(unfinished)?;
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
