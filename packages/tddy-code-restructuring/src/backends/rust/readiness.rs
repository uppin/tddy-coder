//! How the backend decides rust-analyzer is ready to be asked.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use std::time::Instant;

use serde_json::{json, Value};

use super::{
    first_symbol_position, path_of, seam_refusal, server_defect, LspPoint, RustBackend,
    INDEXING_POLL,
};
use crate::{RestructureError, Result};

/// The code rust-analyzer gives the diagnostic it attaches to code a `#[cfg]` has switched off.
const INACTIVE_CODE: &str = "inactive-code";

/// Where a wait for one position ended.
pub(super) enum Answerable {
    /// The server resolves names here.
    Ready,
    /// The server reports the code here as inactive, quoting its own reason. It resolves no name in
    /// such code however long it is given, so this ends a wait as surely as [`Answerable::Ready`].
    Inactive(String),
}

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
    ///
    /// Code rust-analyzer treats as inactive is refused rather than waited on: nothing resolves
    /// there, so an operation acting at it has nothing to act on.
    pub(super) fn wait_until_resolved(&mut self, uri: &str, position: &Value) -> Result<()> {
        match self.wait_until_answerable(uri, position)? {
            Answerable::Ready => Ok(()),
            Answerable::Inactive(said) => Err(inactive_code_refusal(uri, position, &said)?),
        }
    }

    /// Block until the server either resolves names at `position` or says it never will.
    ///
    /// A hover is `null` in two cases, and only one of them passes. Until the crate graph is loaded,
    /// nothing resolves anywhere. Once it is, a `null` can also mean the position is in code a
    /// `#[cfg]` has switched off, which rust-analyzer lists in the outline but never resolves. The
    /// wait once treated both cases as the first and polled until its caller gave up: lifecycle plan
    /// 02a's `check --deep` never finished, on `pty_runtime.rs`'s
    /// `#[cfg(not(unix))] fn resolve_final_argv_env` on macOS. The server does say which case it
    /// is, through its pull diagnostics, and that answer is asked for only once the index is loaded.
    /// Before that, a missing `inactive-code` diagnostic proves nothing.
    pub(super) fn wait_until_answerable(
        &mut self,
        uri: &str,
        position: &Value,
    ) -> Result<Answerable> {
        (self.progress)("waiting for type inference at the anchor");
        let started = Instant::now();
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": position }),
            )?;

            if !hover.is_null() && !self.chatter.loading() {
                self.indexed = true;
                self.refuse_degraded_index()?;
                return Ok(Answerable::Ready);
            }
            if hover.is_null() && self.indexed && !self.chatter.loading() {
                if let Some(said) = self.inactive_code_at(uri, position)? {
                    self.refuse_degraded_index()?;
                    return Ok(Answerable::Inactive(said));
                }
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(self.incomplete_index(started.elapsed()));
            }
        }
    }

    /// The server's reason, when it reports the code at `position` as inactive.
    fn inactive_code_at(&mut self, uri: &str, position: &Value) -> Result<Option<String>> {
        let report = self.request_settled(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        Ok(inactive_code_covering(
            &report,
            &LspPoint::read(Some(position))?,
        ))
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

/// The message of the `inactive-code` diagnostic covering `point`, in a pull-diagnostics report.
fn inactive_code_covering(report: &Value, point: &LspPoint) -> Option<String> {
    let at = (point.line, point.character);
    report
        .get("items")?
        .as_array()?
        .iter()
        .filter(|item| item.get("code").and_then(Value::as_str) == Some(INACTIVE_CODE))
        .find(|item| {
            let bound = |pointer: &str| {
                LspPoint::read(item.pointer(pointer))
                    .ok()
                    .map(|bound| (bound.line, bound.character))
            };
            match (bound("/range/start"), bound("/range/end")) {
                (Some(start), Some(end)) => (start..=end).contains(&at),
                _ => false,
            }
        })
        .map(|item| {
            item.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
}

/// Where a position is, as `path:line`, for a message a reader has to find it from.
pub(super) fn located(uri: &str, position: &Value) -> Result<String> {
    let point = LspPoint::read(Some(position))?;
    Ok(format!("{}:{}", path_of(uri)?.display(), point.line + 1))
}

fn inactive_code_refusal(uri: &str, position: &Value, said: &str) -> Result<RestructureError> {
    Ok(seam_refusal(format!(
        "{} is code rust-analyzer treats as inactive (\"{said}\"), so it resolves nothing there. \
         Run against a server configured with the cfg that activates it, or leave that code out \
         of the operation.",
        located(uri, position)?
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rust-analyzer's own answer for `#[cfg(not(rust_analyzer))] fn …` spanning lines 9–12.
    fn a_report_of_inactive_code_on_lines_9_to_12() -> Value {
        json!({
            "kind": "full",
            "items": [{
                "range": {
                    "start": { "line": 9, "character": 0 },
                    "end": { "line": 12, "character": 1 }
                },
                "severity": 4,
                "code": "inactive-code",
                "message": "code is inactive due to #[cfg] directives: rust_analyzer is enabled"
            }]
        })
    }

    #[test]
    fn reads_the_server_s_reason_at_a_position_inside_inactive_code() {
        // Given
        let report = a_report_of_inactive_code_on_lines_9_to_12();

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 10,
                character: 3,
            },
        );

        // Then
        assert_eq!(
            said.as_deref(),
            Some("code is inactive due to #[cfg] directives: rust_analyzer is enabled")
        );
    }

    #[test]
    fn finds_nothing_at_a_position_outside_inactive_code() {
        // Given
        let report = a_report_of_inactive_code_on_lines_9_to_12();

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 5,
                character: 3,
            },
        );

        // Then
        assert_eq!(said, None);
    }

    #[test]
    fn ignores_a_diagnostic_that_is_not_about_inactive_code() {
        // Given a warning covering the position, of another kind
        let report = json!({
            "kind": "full",
            "items": [{
                "range": {
                    "start": { "line": 9, "character": 0 },
                    "end": { "line": 12, "character": 1 }
                },
                "code": "unused_variables",
                "message": "unused variable"
            }]
        });

        // When
        let said = inactive_code_covering(
            &report,
            &LspPoint {
                line: 10,
                character: 3,
            },
        );

        // Then
        assert_eq!(said, None);
    }
}
