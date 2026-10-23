//! Verify for an installed extension's language (`#reshape` extension-code-actions, R5, PRD F1 a):
//! a change holds when its language server reports no error diagnostic after it that it did not
//! report before, and the language's grammar parses the result with no `ERROR` or `MISSING` node.
//!
//! The run's [`SourceVerify`]: the front end builds one per installed language and hands them to
//! `restructure verify` through `Options::verifies`; Rust files keep Rust's statement comparison.

use std::path::Path;
use std::sync::Arc;

use tddy_code_restructuring::backends::server::file_uri_under;
use tddy_code_restructuring::verify::{ParseNode, SourceChange, SourceFinding, SourceVerify};
use tddy_code_restructuring::{LanguageSupport, Position, RestructureError};
use tddy_lsp::{Diagnostic, LanguageId, LspClient, Range};
use tree_sitter::{Node, Tree};

use crate::support::ExtensionSupport;

/// One extension language's verify.
pub struct ExtensionVerify {
    support: ExtensionSupport,
    /// The language's running server, whose diagnostics are compared; `None` for a language that
    /// runs no server, whose verify is the parse alone (there are no diagnostics to compare).
    server: Option<Arc<LspClient>>,
}

impl ExtensionVerify {
    /// The verify of `support`'s language, comparing the diagnostics of `server` when it runs one.
    pub fn new(support: ExtensionSupport, server: Option<Arc<LspClient>>) -> Self {
        Self { support, server }
    }
}

impl ExtensionVerify {
    /// One verify per support, each comparing the diagnostics of the server `servers` holds for its
    /// language and judging by the parse alone when it holds none.
    pub fn for_languages(
        supports: &[ExtensionSupport],
        servers: &[(LanguageId, Arc<LspClient>)],
    ) -> Vec<Arc<dyn SourceVerify>> {
        supports
            .iter()
            .map(|support| {
                let server = servers
                    .iter()
                    .find(|(language, _)| language == support.id())
                    .map(|(_, client)| Arc::clone(client));
                Arc::new(Self::new(support.clone(), server)) as Arc<dyn SourceVerify>
            })
            .collect()
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
        let mut findings = Vec::new();
        if let Some(server) = &self.server {
            let diagnostics = self.new_diagnostics(server, root, change)?;
            findings.extend(diagnostics.into_iter().map(|diagnostic| {
                SourceFinding::NewErrorDiagnostic {
                    path: change.path.clone(),
                    at: one_based(
                        diagnostic.range.start.line,
                        diagnostic.range.start.character,
                    ),
                    message: diagnostic.message,
                }
            }));
        }
        findings.extend(self.parse_findings(change));
        Ok(findings)
    }
}

/// Where a server advertises that it answers `textDocument/diagnostic`.
const PULL_DIAGNOSTICS_CAPABILITY: &str = "/capabilities/diagnosticProvider";

/// Where a text the parser gives up on altogether is reported.
const TEXT_START: Position = Position { line: 1, col: 1 };

/// A zero-based line and column (an LSP position, a tree-sitter point) as a one-based [`Position`].
fn one_based(line: u32, col: u32) -> Position {
    Position {
        line: line + 1,
        col: col + 1,
    }
}

impl ExtensionVerify {
    /// The error diagnostics `server` reports for `change.after` that it did not report for
    /// `change.before`, each read by a pull request after the text was announced — so the answer is
    /// for that text and not for whatever the server last published. The document is closed again
    /// whatever the reads answered, handing the file back to what is on disk.
    fn new_diagnostics(
        &self,
        server: &LspClient,
        root: &Path,
        change: &SourceChange,
    ) -> tddy_code_restructuring::Result<Vec<Diagnostic>> {
        self.require_pull_diagnostics(server)?;
        let uri = file_uri_under(root, &change.path);
        let language_id = self.support.document_language_id();
        let read = tokio::runtime::Handle::current().block_on(async {
            let read = diagnostics_before_and_after(server, &uri, language_id, change).await;
            let closed = server.did_close(&uri).await;
            read.and_then(|both| closed.map(|()| both))
        });
        let (before, after) = read.map_err(|error| {
            RestructureError::ServerDefect(format!(
                "reading diagnostics of {}: {error}",
                change.path
            ))
        })?;
        Ok(new_error_diagnostics(
            &before,
            &after,
            &change.before,
            &change.after,
        ))
    }

    /// Refuses a server that cannot be asked for the diagnostics of the text last sent to it: the
    /// publish cache says nothing about that text, so a verify reading it would hold unchecked.
    fn require_pull_diagnostics(&self, server: &LspClient) -> tddy_code_restructuring::Result<()> {
        if server
            .handshake()
            .pointer(PULL_DIAGNOSTICS_CAPABILITY)
            .is_some()
        {
            return Ok(());
        }
        Err(RestructureError::ServerDefect(format!(
            "the language server of {} offers no pull diagnostics, so the errors a change \
             introduces cannot be read from it",
            self.support.id()
        )))
    }

    /// One [`SourceFinding::ParseError`] per `ERROR` / `MISSING` node of the grammar's parse of
    /// `change.after`; a text the parser gives up on altogether is one `ERROR` at its start.
    fn parse_findings(&self, change: &SourceChange) -> Vec<SourceFinding> {
        let found = match self.support.parse(&change.after) {
            Some(tree) => parse_errors(&tree),
            None => vec![(ParseNode::Error, TEXT_START)],
        };
        found
            .into_iter()
            .map(|(node, at)| SourceFinding::ParseError {
                path: change.path.clone(),
                node,
                at,
            })
            .collect()
    }
}

/// `change.before`, then `change.after`, announced as the text of `uri` and the diagnostics of each
/// pulled back. Leaves the document open; the caller closes it.
async fn diagnostics_before_and_after(
    server: &LspClient,
    uri: &str,
    language_id: &str,
    change: &SourceChange,
) -> Result<(Vec<Diagnostic>, Vec<Diagnostic>), tddy_lsp::LspError> {
    server
        .sync_document(uri, language_id, &change.before)
        .await?;
    let before = server.pull_diagnostics(uri).await?;
    server
        .sync_document(uri, language_id, &change.after)
        .await?;
    let after = server.pull_diagnostics(uri).await?;
    Ok((before, after))
}

/// The error diagnostics (severity 1) of `after` that `before` does not account for: one of
/// `before` accounts for one of `after` when their messages are equal and its range, carried from
/// `before_text` to `after_text` through the lines the change left alone, is the same range. A
/// warning is never counted, and a diagnostic the change removed is not a finding.
pub(crate) fn new_error_diagnostics(
    before: &[Diagnostic],
    after: &[Diagnostic],
    before_text: &str,
    after_text: &str,
) -> Vec<Diagnostic> {
    let carry = LineCarry::between(before_text, after_text);
    let mut carried: Vec<(&str, Range)> = before
        .iter()
        .filter(|diagnostic| diagnostic.severity == ERROR_SEVERITY)
        .filter_map(|diagnostic| {
            Some((diagnostic.message.as_str(), carry.range(diagnostic.range)?))
        })
        .collect();
    let mut new = Vec::new();
    for diagnostic in after
        .iter()
        .filter(|diagnostic| diagnostic.severity == ERROR_SEVERITY)
    {
        let accounted_for = carried.iter().position(|(message, range)| {
            *message == diagnostic.message && *range == diagnostic.range
        });
        match accounted_for {
            Some(index) => {
                carried.remove(index);
            }
            None => new.push(diagnostic.clone()),
        }
    }
    new
}

/// LSP's severity for an error.
const ERROR_SEVERITY: u8 = 1;

/// Where a line of the text before a change is in the text after it, for the lines the change left
/// alone: those of the common prefix, and those of the common suffix shifted by how many lines the
/// change added.
struct LineCarry {
    prefix: u32,
    before_lines: u32,
    after_lines: u32,
    suffix: u32,
}

impl LineCarry {
    fn between(before: &str, after: &str) -> Self {
        let before: Vec<&str> = before.split('\n').collect();
        let after: Vec<&str> = after.split('\n').collect();
        let shorter = before.len().min(after.len());
        let prefix = before
            .iter()
            .zip(&after)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = before
            .iter()
            .rev()
            .zip(after.iter().rev())
            .take(shorter - prefix)
            .take_while(|(a, b)| a == b)
            .count();
        Self {
            prefix: prefix as u32,
            before_lines: before.len() as u32,
            after_lines: after.len() as u32,
            suffix: suffix as u32,
        }
    }

    /// The line `line` of the text before, as numbered in the text after; `None` for a line the
    /// change touched.
    fn line(&self, line: u32) -> Option<u32> {
        if line < self.prefix {
            Some(line)
        } else if line >= self.before_lines - self.suffix {
            Some(line + self.after_lines - self.before_lines)
        } else {
            None
        }
    }

    /// `range` of the text before, carried to the text after; `None` when either end is on a line
    /// the change touched.
    fn range(&self, range: Range) -> Option<Range> {
        let carried = |at: tddy_lsp::Position| {
            Some(tddy_lsp::Position::at(self.line(at.line)?, at.character))
        };
        Some(Range {
            start: carried(range.start)?,
            end: carried(range.end)?,
        })
    }
}

/// Every `ERROR` and `MISSING` node of `tree`, outermost first in source order, with where it
/// starts (one-based line, one-based byte column). Walked with a cursor rather than by recursion,
/// so a deeply nested error does not grow the stack; a subtree the parser found nothing wrong in is
/// not entered.
pub(crate) fn parse_errors(tree: &Tree) -> Vec<(ParseNode, Position)> {
    let mut found = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let wrong = node.has_error() || node.is_missing();
        if wrong {
            if let Some(kind) = parse_node_of(node) {
                let start = node.start_position();
                found.push((kind, one_based(start.row as u32, start.column as u32)));
            }
        }
        if wrong && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

/// What `node` is, when it is one a verify reports.
fn parse_node_of(node: Node<'_>) -> Option<ParseNode> {
    if node.is_error() {
        Some(ParseNode::Error)
    } else if node.is_missing() {
        Some(ParseNode::Missing)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
    fn an_error_on_an_inserted_line_is_new_and_the_carried_one_is_not() {
        // Given an error on line 1 before, and after a line was inserted below it, the same error
        // on line 1 and another with the same message on the inserted line
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
