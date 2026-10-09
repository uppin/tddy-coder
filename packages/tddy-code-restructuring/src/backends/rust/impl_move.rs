//! `move_impl_members`: moving a run of members of one inherent `impl T` into an `impl T` block of
//! another module of the same crate.
//!
//! rust-analyzer has no assist for this, and `extract_module`'s inherent-member path can only write
//! a new child module of the file and widens every private member to `pub(crate)`. Like
//! `move_item`, this operation is authored here and informed by the server: where the members are
//! comes from the outline, who calls them from `textDocument/references`, and everything else is a
//! function of the files' text, in [`assemble`]. No caller is edited — a method resolves through its
//! type wherever its block is written. See `docs/dev/1-WIP/2026-10-09-reshape-move-impl-members.md`
//! for the rules (R1–R7) and the refusals (P1–P9, S1–S6).

pub(super) mod assemble;
pub(super) mod landing;

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// The refusal every entry point makes until the operation is implemented.
fn not_implemented_yet() -> crate::RestructureError {
    crate::RestructureError::UnsupportedOp {
        backend: "`move_impl_members` is not implemented yet (#reshape 13/19)".to_string(),
        op: "MoveImplMembers".to_string(),
    }
}

/// What a static check finds wrong with a `move_impl_members`, from the plan and the text alone.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    // TODO(reshape-move-impl-members): implement P2–P5 (changeset § Plan-line schema).
    let _ = (op, workspace);
    Err(not_implemented_yet())
}

impl RustBackend {
    /// Resolve a `move_impl_members` into the edits to every file it changes.
    pub(super) fn move_impl_members(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        // TODO(reshape-move-impl-members): implement — read the run, survey, land, assemble.
        let _ = (op, workspace);
        Err(not_implemented_yet())
    }
}
