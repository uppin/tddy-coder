//! `read_fields_through`: rebinding the `self` a range of a method reads to a local inserted before
//! it, so the range names no `self` and can be extracted as a free function.
//!
//! **Field mode** (`expr` is a borrowed view of the host's fields, `self.agent_roster_state()`):
//! every `self.<field>` in the range becomes `<name>.<field>`, each field is checked against the
//! view's type on a trial text, and a `&` the view already provides is dropped. **Self mode**
//! (`expr` is `self`, `&*self` or `&mut *self`): every `self` becomes `<name>`, method receivers
//! included, and every `Self` becomes the enclosing impl's self type. See
//! `docs/dev/1-WIP/2026-10-09-reshape-methods-leave-type.md` for the rules (RS1-RS7).
//!
//! `range` and `edits` are `pub(crate)`: `#reshape` 18's `detach_method` reuses the self-mode
//! rewrite and the shadowing refusal from a sibling module.

#[allow(
    dead_code,
    reason = "TODO(reshape-methods-leave-type): the run calls these in the green phase"
)]
pub(crate) mod edits;
#[allow(
    dead_code,
    reason = "TODO(reshape-methods-leave-type): the run calls these in the green phase"
)]
pub(crate) mod range;
#[allow(
    dead_code,
    reason = "TODO(reshape-methods-leave-type): the run calls these in the green phase"
)]
mod typing;

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `read_fields_through`, from the plan and the text alone:
/// RS1-RS4 for a `range` anchor.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    // TODO(reshape-methods-leave-type): implement — `range::lexical_refusals` over the anchored range.
    let _ = (op, workspace);
    Err(not_implemented())
}

impl RustBackend {
    /// Resolve a `read_fields_through` into the edit to the one file that holds the range.
    pub(super) fn read_fields_through(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        // TODO(reshape-methods-leave-type): implement — RS1-RS7, the trial text, the edit, the note.
        let _ = (op, workspace);
        Err(not_implemented())
    }
}

/// The refusal that stands in for the operation until it is implemented.
fn not_implemented() -> crate::RestructureError {
    crate::RestructureError::UnsupportedOp {
        backend: "`read_fields_through` is not implemented yet (TODO(reshape-methods-leave-type))"
            .to_string(),
        op: "ReadFieldsThrough".to_string(),
    }
}
