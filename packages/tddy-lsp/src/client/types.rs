//! The domain shapes this client hands its callers: positions and ranges in LSP semantics,
//! locations, diagnostics, symbols, and the error object a server sends in place of a result.
//! Deliberately not `lsp-types`' own structs — a caller should not have to depend on that crate's
//! version to read an answer.

/// A zero-based line/character position (LSP semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    /// A position at `line`/`character`.
    pub fn at(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A half-open range between two positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// A location: a document URI plus a range within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// A diagnostic (error/warning/…) at a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    /// LSP severity: 1=Error, 2=Warning, 3=Information, 4=Hint.
    pub severity: u8,
    pub message: String,
    pub source: Option<String>,
}

/// A symbol reported by document/workspace symbol requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    /// LSP `SymbolKind` numeric code.
    pub kind: u8,
    pub location: Location,
    pub container: Option<String>,
}

/// A JSON-RPC `error` object the server sent in place of a `result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseError {
    /// The server's own error code (e.g. -32801 `ContentModified`).
    pub code: i64,
    pub message: String,
}
