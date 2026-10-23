//! `apply_code_action` on a Rust file (`#reshape` extension-code-actions, PRD F2 a): the anchor's
//! range, the document opened as `rust`, and the rest through [`crate::backends::code_action`] — the
//! same path an extension language's server takes, so there is one selection and one resolution
//! rule for every server, rust-analyzer included.

use super::RustBackend;
use crate::backends::code_action;
use crate::backends::LspClientBridge;
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
        self.shared_client()?;
        let relative = op.anchor.file();
        let uri = super::uri_of(&workspace.root.join(relative));
        let original = workspace.read(relative)?;

        self.start(workspace.root)?;
        self.did_open(&uri, &original)?;
        self.ensure_indexed(&uri)?;
        let range = self.anchor_range(&uri, op)?;
        (self.progress)("code action: asking rust-analyzer what it offers over the range");
        code_action::resolve(self.shared_client()?, op, range, &[], workspace)
    }

    /// The shared language-server client this backend was handed. A backend that spawned its own
    /// server has none, and `apply_code_action` is refused rather than run on a transport that
    /// cannot carry `workspace/applyEdit`.
    fn shared_client(&self) -> Result<&LspClientBridge> {
        self.bridge.as_ref().ok_or_else(|| {
            super::failure(
                "apply_code_action runs on a shared language-server client, and this backend \
                 spawned its own server",
            )
        })
    }
}
