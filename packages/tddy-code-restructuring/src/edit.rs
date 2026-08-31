//! The edit vocabulary shared by every backend. Deliberately mirrors the LSP `WorkspaceEdit`
//! shape, because both language engines already speak it.

use serde::{Deserialize, Serialize};

/// A one-based line/column position, as editors and language servers report them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// A single replacement within one file. `new_text` originates from the language engine —
/// never from a plan.
///
/// The wire name is camelCase because the TypeScript sidecar speaks this shape too; each side
/// keeps its own naming convention and the contract is pinned here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}

/// What happens to one file as part of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileEdit {
    /// Text replacements inside an existing file.
    Change { path: String, edits: Vec<TextEdit> },
    /// A file the engine brought into existence as a consequence of a refactor.
    Create { path: String },
    /// A move. Applied with `git mv` so history survives.
    Rename { from: String, to: String },
}

/// The complete, multi-file result of resolving one operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub changes: Vec<FileEdit>,
}

/// One item's visibility as the plan found it, and as the extraction left it.
///
/// An extraction widens what it relocates. Where a reference genuinely requires that, the widening
/// stands — but it is an *output* of the operation, not an implementation detail, so it is reported
/// and journalled rather than discovered later by reading the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityChange {
    pub item: String,
    pub from: String,
    pub to: String,
}

/// What resolving one operation produced: the edit, and what the backend has to say about it.
///
/// The edit alone was the whole return value once. A backend that has something to report — a
/// visibility it could not preserve — had nowhere to put it, so widening the return type is what
/// gives every backend, present and future, somewhere for it to go.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub edit: WorkspaceEdit,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report: Vec<VisibilityChange>,
    /// Anything else the backend has to say about the operation, in the words it wants said — a
    /// facade it was asked for and had nothing to put in. A visibility change has a shape worth
    /// naming; a note is prose precisely because the next one will not share this one's shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Resolution {
    /// A resolution with nothing to report, which is what most operations produce.
    pub fn of(edit: WorkspaceEdit) -> Resolution {
        Resolution {
            edit,
            report: Vec::new(),
            notes: Vec::new(),
        }
    }
}
