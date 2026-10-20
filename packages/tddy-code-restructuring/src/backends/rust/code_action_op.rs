//! `apply_code_action` on a Rust file (`#reshape` extension-code-actions, PRD F2 a): the anchor's
//! range, the document opened as `rust`, and the rest through [`crate::backends::code_action`] — the
//! same path an extension language's server takes, so there is one selection and one resolution
//! rule for every server, rust-analyzer included.

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

impl RustBackend {
    /// Resolve `op`, an `apply_code_action`, to the edit of the one action rust-analyzer offers
    /// over its anchor that `op.action` selects (R1–R2). Asks for every kind: Rust's support
    /// declares none to narrow by.
    pub(super) fn apply_code_action(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let _ = (op, workspace);
        // TODO(reshape-extension-code-actions): implement
        todo!("RustBackend::apply_code_action")
    }
}
