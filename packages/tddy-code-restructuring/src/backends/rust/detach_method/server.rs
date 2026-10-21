//! rust-analyzer's answers to `detach_method`: the [`DetachServer`] seam for [`RustBackend`].
//!
//! Each answer is the server's own shape, read the way the other authored operations read it: the
//! anchor's span through `span_of`, the settled `documentSymbol` outline, the `textDocument/references`
//! set and the `textDocument/hover` of a receiver. A document is opened once per operation, at the
//! text the workspace holds for it, and the entry point that started the operation closes it.

use serde_json::{json, Value};

use super::super::lsp_edits::position_at;
use super::super::{character_column, relative_to, server_defect, uri_of, LspPoint, RustBackend};
use super::scan;
use super::DetachServer;
use crate::crate_move::Reference;
use crate::edit::{Position, Range};
use crate::item_anchor::span_of;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

impl DetachServer for RustBackend {
    fn span(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Range> {
        let range = span_of(&op.anchor, workspace.root, self)?;
        // `span_of` opens and closes the documents it reads, and the close hands back the projection
        // the run staged for this operation's own documents, so it is staged again for them.
        self.stage_projection(workspace)?;
        Ok(range)
    }

    fn outline(&mut self, workspace: &Workspace<'_>, file: &str) -> Result<Value> {
        let uri = self.open_document(workspace, file)?;
        self.settled_outline(&uri)
    }

    fn references(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
        at: Position,
    ) -> Result<Vec<Reference>> {
        let uri = self.open_document(workspace, file)?;
        let text = workspace.read(file)?;
        let position = position_at(&text, scan::offset_of(&text, at));
        let answer = self.references_at(&uri, &position)?;

        let mut found = Vec::new();
        for reference in answer.as_array().into_iter().flatten() {
            let referrer = reference
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| server_defect("a reference carries no uri"))?;
            let path = relative_to(referrer, workspace.root)?;
            let referring = workspace.read(&path)?;
            let at = character_column(
                &referring,
                LspPoint::read(reference.pointer("/range/start"))?,
            );
            found.push(Reference { path, at });
        }
        Ok(found)
    }

    fn hover(&mut self, workspace: &Workspace<'_>, file: &str, at: Position) -> Result<Value> {
        let uri = self.open_document(workspace, file)?;
        let text = workspace.read(file)?;
        let position = position_at(&text, scan::offset_of(&text, at));
        self.wait_until_resolved(&uri, &position)?;
        self.request_settled(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": position }),
        )
    }
}

impl RustBackend {
    /// Start the server for this workspace, open `file` at the text the workspace holds for it,
    /// unless this operation already has it open, and warm the index. The document's uri.
    fn open_document(&mut self, workspace: &Workspace<'_>, file: &str) -> Result<String> {
        let uri = uri_of(&workspace.root.join(file));
        self.start(workspace.root)?;
        if !self.opened.iter().any(|open| open == &uri) {
            let text = workspace.read(file)?;
            self.did_open(&uri, &text)?;
        }
        self.ensure_indexed(&uri)?;
        Ok(uri)
    }
}
