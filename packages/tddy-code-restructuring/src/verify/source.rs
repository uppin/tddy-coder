//! Verify for a language tddy has no passes of its own for — an installed extension's (`#reshape`
//! extension-code-actions, R5, PRD F1 a).
//!
//! The statement multiset this module's parent holds a Rust tree to is Rust's alone. For any other
//! language a run is verified by what that language already knows about the file: the server's
//! error diagnostics after the run are a subset of those before (by message, their ranges mapped
//! through the change), and the grammar parses the file with no `ERROR` or `MISSING` node. This
//! crate cannot load a grammar or start an extension's server, so the definition is a
//! [`SourceVerify`] the front end hands the run (`tddy_zed_extensions::ExtensionVerify`), and the
//! run hands it each changed file its verify claims — see `runner::verify_sources`.

use std::path::Path;

use crate::edit::Position;
use crate::Result;

/// One file as it was before the change and as it is after, its path relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChange {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// The node the grammar put where it could not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseNode {
    /// Text the grammar could not place (`ERROR`).
    Error,
    /// A token the grammar expected and did not find (`MISSING`).
    Missing,
}

/// Why a changed file does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFinding {
    /// An error diagnostic the server reports after the change that it did not report before.
    NewErrorDiagnostic {
        path: String,
        /// Where it starts, one-based.
        at: Position,
        message: String,
    },
    /// A node of the parse after the change the grammar could not parse.
    ParseError {
        path: String,
        node: ParseNode,
        /// Where it starts, one-based.
        at: Position,
    },
}

/// A language's verify: which files it judges, and what it finds in one change of one of them.
pub trait SourceVerify: Send + Sync {
    /// Whether `path`, relative to the root, is a file this verify judges.
    fn claims(&self, path: &Path) -> bool;

    /// What `change` leaves wrong, in the tree at `root`: empty when it holds.
    fn verify(&self, root: &Path, change: &SourceChange) -> Result<Vec<SourceFinding>>;
}
