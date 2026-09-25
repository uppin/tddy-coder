//! The documents this backend puts in front of the server, and handing each one back.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.
//!
//! An open document is the server's authority on that file: rust-analyzer stops reading it from
//! disk until it is closed. So every document an operation opens is closed when the operation ends,
//! whatever it ended with. A server is often shared: `tddy-index-daemon` keeps one per root for as
//! long as it runs, and it serves backend after backend. A document left open outlives the run that
//! opened it, and it goes on answering for its file with whatever text that run sent last. That is
//! usually a trial import or a rehearsed text, not the tree.
//!
//! That is how plan 05 of the lifecycle destructure was refused three times over a correct path. An
//! earlier `check --deep` of plan 01 had left `connection_service.rs` open at a 999-line rehearsed
//! text of a 1,652-line file. Its seams had moved into modules whose files existed only in that
//! check's overlay, so for the server `connection_service::SplitStartFailure` was declared nowhere,
//! and `use super::super::SplitStartFailure;` resolved to nothing.

use serde_json::json;

use super::RustBackend;
use crate::Result;

impl RustBackend {
    /// Open `uri` at `text`, and remember that it is this backend's to close.
    pub(super) fn did_open(&mut self, uri: &str, text: &str) -> Result<()> {
        if !self.opened.iter().any(|open| open == uri) {
            self.opened.push(uri.to_string());
        }
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "rust", "version": 1, "text": text } }),
        )
    }

    /// Run one entry point, then close every document it opened, whatever it returned.
    ///
    /// When both fail, the entry point's own error is the one returned: it says what happened to
    /// the operation, and a close that failed after it only says the transport has gone too.
    pub(super) fn closing_what_it_opens<T>(
        &mut self,
        entry_point: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let outcome = entry_point(self);
        let closed = self.close_opened();
        let value = outcome?;
        closed.map(|()| value)
    }

    /// Close every document this backend has open, handing each file back to what is on disk.
    fn close_opened(&mut self) -> Result<()> {
        for uri in std::mem::take(&mut self.opened) {
            self.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri } }),
            )?;
        }
        Ok(())
    }
}
