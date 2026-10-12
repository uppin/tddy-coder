//! `detach_method`: turning the members of one inherent `impl` block into free functions over a
//! named parameter, and re-pointing every caller the server knows.
//!
//! The function is written in the member's module with its visibility, doc comments and attributes
//! unchanged — an inherent method's visibility is relative to the module of its block, so the
//! function reaches exactly the callers the method did and nothing is widened. `self` becomes the
//! parameter (`read_fields_through`'s self-mode rewrite) and `Self` the block's self type. A method
//! call `R.m(a)` becomes `m(R, a)` (or `M::m(R, a)` from another module), a path call `Self::m(a)`
//! becomes `m(a)`, and a block the run empties is removed. See
//! `docs/dev/1-WIP/2026-10-09-reshape-backend-session.md` for the rules (DP1-DP3, DS1-DS10).
//!
//! Its resolver is a free function over the backend from birth: the shape every operation of the
//! Rust backend takes once `#reshape` 18 has detached them.

#[allow(
    dead_code,
    reason = "TODO(reshape-backend-session): the run calls these in the green phase"
)]
mod calls;
#[allow(
    dead_code,
    reason = "TODO(reshape-backend-session): the run calls these in the green phase"
)]
mod text;
#[allow(
    dead_code,
    reason = "TODO(reshape-backend-session): the run calls these in the green phase"
)]
mod typing;

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `detach_method`, from the plan and the text alone.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    // TODO(reshape-backend-session): implement — DS1-DS7 over the anchored members' text.
    let _ = (op, workspace);
    Err(not_implemented())
}

/// Resolve a `detach_method` into the edits to the file that holds the block and to every file that
/// calls one of its members.
pub(super) fn detach_method(
    session: &mut RustBackend,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    // TODO(reshape-backend-session): implement — the members, DS1-DS10, the functions, the callers,
    // the emptied block, the note.
    let _ = (session, op, workspace);
    Err(not_implemented())
}

/// The refusal that stands in for the operation until it is implemented.
fn not_implemented() -> crate::RestructureError {
    crate::RestructureError::UnsupportedOp {
        backend: "`detach_method` is not implemented yet (TODO(reshape-backend-session))"
            .to_string(),
        op: "DetachMethod".to_string(),
    }
}
