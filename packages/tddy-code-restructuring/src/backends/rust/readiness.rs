//! How the backend decides rust-analyzer is ready to be asked.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use std::time::Instant;

use serde_json::{json, Value};

use super::{first_symbol_position, server_defect, RustBackend, INDEXING_POLL};
use crate::Result;

impl RustBackend {
    /// Wait, once per process, for the crate graph to load — with the server's progress on screen.
    ///
    /// Every request that needs name resolution is answered emptily until rust-analyzer has loaded
    /// the graph, so waiting for it here once is what keeps every later wait short. On a real crate
    /// the alternative is paid per operation, and `survey_moved_items` pays it per moved item.
    ///
    /// Hover is the authority, because it is the cheapest request that needs the graph and it is the
    /// same signal a rename is gated on. `serverStatus` is a shortcut out when it says quiescent,
    /// and a reason to keep waiting while it says loading: hover answers before build scripts and
    /// proc macros have loaded, and an extraction taken then writes `req: _` for a type generated
    /// into `OUT_DIR`, while the import pass finds no unresolved name to restore. It is an
    /// extension, so a server that never sends it still gets past this on hover alone.
    ///
    /// A document with no symbols has nothing to hover, so the warm-up is skipped rather than spent
    /// on a position that would never resolve — which leaves `indexed` false, and the first real
    /// wait doing the waiting instead.
    pub(super) fn ensure_indexed(&mut self, uri: &str) -> Result<()> {
        if self.indexed {
            return self.refuse_degraded_index();
        }
        (self.progress)("warming crate index (until ready, or until you stop waiting)");
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        let Some(probe) = first_symbol_position(&symbols) else {
            (self.progress)("no indexable symbols in file; skipping warm-up");
            return Ok(());
        };

        let started = Instant::now();
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": probe }),
            )?;

            if (!hover.is_null() || self.chatter.quiescent()) && !self.chatter.loading() {
                self.indexed = true;
                (self.progress)("crate index ready");
                return self.refuse_degraded_index();
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(self.incomplete_index(started.elapsed()));
            }
        }
    }

    /// Block until the server can resolve names at `position`.
    ///
    /// `documentSymbol` is answered from the syntax tree and so succeeds immediately, but a rename
    /// needs the crate graph. Hover is the cheapest request that also needs it, so a non-null hover
    /// is the signal that a rename will be accepted — once the server is not still loading, for the
    /// reason [`Self::ensure_indexed`] gives.
    pub(super) fn wait_until_resolved(&mut self, uri: &str, position: &Value) -> Result<()> {
        (self.progress)("waiting for type inference at the anchor");
        let started = Instant::now();
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": position }),
            )?;

            if !hover.is_null() && !self.chatter.loading() {
                self.indexed = true;
                return self.refuse_degraded_index();
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(self.incomplete_index(started.elapsed()));
            }
        }
    }

    /// Refuse to go on once ready, when rust-analyzer has said its index is degraded.
    ///
    /// Ready is not trustworthy: a server whose build scripts failed still finishes loading, and
    /// then answers as though what they generate did not exist — which is how an extraction came
    /// to write `req: _` against an index everyone called warm. Checked at every point readiness is
    /// declared, so it holds for the first wait of a run and for each later one, on a server this
    /// backend started and on a warm one it was handed: the bridge folds in the server's latest
    /// status even when another consumer drained the transition. The reasoning for refusing on
    /// `warning` as well as `error` is at [`super::ServerChatter::degraded`].
    fn refuse_degraded_index(&self) -> Result<()> {
        match self.chatter.degraded() {
            Some(reason) => Err(server_defect(reason)),
            None => Ok(()),
        }
    }
}
