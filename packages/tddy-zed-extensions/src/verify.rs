//! Verify for an installed extension's language (`#reshape` extension-code-actions, R5, PRD F1 a):
//! a change holds when its language server reports no error diagnostic after it that it did not
//! report before, and the language's grammar parses the result with no `ERROR` or `MISSING` node.
//!
//! The run's [`SourceVerify`]: the front end builds one per installed language and hands them to
//! `restructure verify` through `Options::verifies`; Rust files keep Rust's statement comparison.

use std::path::Path;
use std::sync::Arc;

use tddy_code_restructuring::verify::{ParseNode, SourceChange, SourceFinding, SourceVerify};
use tddy_code_restructuring::{LanguageSupport, Position};
use tddy_lsp::{Diagnostic, LspClient};
use tree_sitter::Tree;

use crate::support::ExtensionSupport;

/// One extension language's verify.
pub struct ExtensionVerify {
    support: ExtensionSupport,
    /// The language's running server, whose diagnostics are compared; `None` for a language that
    /// runs no server, whose verify is the parse alone (there are no diagnostics to compare).
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-code-actions): read by verify once implemented"
    )]
    server: Option<Arc<LspClient>>,
}

impl ExtensionVerify {
    /// The verify of `support`'s language, comparing the diagnostics of `server` when it runs one.
    pub fn new(support: ExtensionSupport, server: Option<Arc<LspClient>>) -> Self {
        Self { support, server }
    }
}

impl SourceVerify for ExtensionVerify {
    /// The files the support claims.
    fn claims(&self, path: &Path) -> bool {
        self.support.claims(path)
    }

    /// R5 over one file: with a server, `change.before` then `change.after` are announced as the
    /// document's text (opened as the launch's `languageId`) and the error diagnostics of each read
    /// back — every one after that [`new_error_diagnostics`] does not find before is a
    /// [`SourceFinding::NewErrorDiagnostic`]; then every node of the grammar's parse of
    /// `change.after` that [`parse_errors`] finds is a [`SourceFinding::ParseError`]. Blocks on the
    /// caller's tokio runtime, as the restructuring backends do.
    fn verify(
        &self,
        root: &Path,
        change: &SourceChange,
    ) -> tddy_code_restructuring::Result<Vec<SourceFinding>> {
        let _ = (root, change);
        // TODO(reshape-extension-code-actions): implement
        todo!("ExtensionVerify::verify")
    }
}

/// The error diagnostics (severity 1) of `after` that `before` does not account for: one of
/// `before` accounts for one of `after` when their messages are equal and its range, carried from
/// `before_text` to `after_text` through the lines the change left alone, is the same range. A
/// warning is never counted, and a diagnostic the change removed is not a finding.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "TODO(reshape-extension-code-actions): ExtensionVerify::verify calls it once implemented"
    )
)]
pub(crate) fn new_error_diagnostics(
    before: &[Diagnostic],
    after: &[Diagnostic],
    before_text: &str,
    after_text: &str,
) -> Vec<Diagnostic> {
    let _ = (before, after, before_text, after_text);
    // TODO(reshape-extension-code-actions): implement
    todo!("new_error_diagnostics")
}

/// Every `ERROR` and `MISSING` node of `tree`, outermost first in source order, with where it
/// starts (one-based line, one-based byte column).
#[allow(
    dead_code,
    reason = "TODO(reshape-extension-code-actions): ExtensionVerify::verify calls it once implemented"
)]
pub(crate) fn parse_errors(tree: &Tree) -> Vec<(ParseNode, Position)> {
    let _ = tree;
    // TODO(reshape-extension-code-actions): implement
    todo!("parse_errors")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tddy_lsp::Range;

    /// A diagnostic of `severity` on zero-based `line`, `from..to`, saying `message`.
    fn a_diagnostic(severity: u8, line: u32, from: u32, to: u32, message: &str) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: tddy_lsp::Position::at(line, from),
                end: tddy_lsp::Position::at(line, to),
            },
            severity,
            message: message.to_string(),
            source: Some("fake".to_string()),
        }
    }

    const BEFORE: &str = "[server]\nport = BROKEN\n";

    #[test]
    fn a_diagnostic_moved_down_by_an_inserted_line_is_not_new() {
        // Given an error on line 1 before, and the same error on line 2 after a line was inserted
        // above it
        let after_text = "# config\n[server]\nport = BROKEN\n";
        let before = [a_diagnostic(1, 1, 7, 13, "unexpected BROKEN")];
        let after = [a_diagnostic(1, 2, 7, 13, "unexpected BROKEN")];

        // When the new errors are counted
        let new = new_error_diagnostics(&before, &after, BEFORE, after_text);

        // Then there are none: the change moved it, and did not cause it
        assert_eq!(new, Vec::new());
    }

    #[test]
    fn an_error_with_a_message_not_reported_before_is_new() {
        // Given no error before, and one after
        let after_text = "[server]\nport = BROKEN\nhost = BROKEN\n";
        let before = [a_diagnostic(1, 1, 7, 13, "unexpected BROKEN")];
        let after = [
            a_diagnostic(1, 1, 7, 13, "unexpected BROKEN"),
            a_diagnostic(1, 2, 7, 13, "unexpected BROKEN"),
        ];

        // When the new errors are counted
        let new = new_error_diagnostics(&before, &after, BEFORE, after_text);

        // Then the one on the inserted line is new, the one carried over is not
        assert_eq!(new, vec![a_diagnostic(1, 2, 7, 13, "unexpected BROKEN")]);
    }

    #[test]
    fn a_new_warning_is_not_counted() {
        // Given a warning that appears after the change
        let after = [a_diagnostic(2, 0, 0, 8, "unused table")];

        // When the new errors are counted
        let new = new_error_diagnostics(&[], &after, BEFORE, BEFORE);

        // Then a warning is not an error
        assert_eq!(new, Vec::new());
    }
}
