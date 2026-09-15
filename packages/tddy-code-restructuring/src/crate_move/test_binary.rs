//! Moving a test binary — `<crate>/tests/<name>.rs` — to the crate whose code it exercises.

use crate::crate_move::destination::Destination;
use crate::edit::WorkspaceEdit;
use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::Result;

/// Where a test binary sits, and where it is going.
///
/// Deliberately not [`Move`]: that struct carries `module`, `origin` and `reexport`, and a test
/// binary has no module name to declare, no `mod` line in any origin to remove, and no facade it
/// could ever leave behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestBinaryMove {
    /// The test file, relative to the repository root — `<crate>/tests/<name>.rs`.
    pub source: String,
    /// The binary's name, which is its file stem. Cargo derives it the same way.
    pub name: String,
    /// The crate the test is leaving. Needed only to resolve its `use` header.
    pub origin: Destination,
    /// The crate whose code it actually exercises.
    pub destination: Destination,
}

impl TestBinaryMove {
    /// Where the file lands.
    #[must_use]
    pub fn moved_to(&self) -> String {
        format!("{}/tests/{}.rs", self.destination.dir, self.name)
    }
}

/// Read a test-binary move from its operation.
///
/// # Errors
///
/// Refuses an anchor that is not `<crate>/tests/<name>.rs` — a module move and a test-binary move
/// are different operations precisely because the path shapes differ, so admitting the wrong one
/// here would produce an edit neither operation's rules cover.
pub fn read_test_binary_move(
    _workspace: &Workspace<'_>,
    _op: &RefactorOp,
) -> Result<TestBinaryMove> {
    // TODO(test-homes): implement
    todo!("read_test_binary_move: resolve a `<crate>/tests/<name>.rs` anchor")
}

/// Resolve the move of a test binary to the crate whose code it exercises.
///
/// Three edits, and no more: the rename, the moved file's own `use` header, and the destination's
/// `[dev-dependencies]`. There is no origin edit at all — cargo auto-discovers `tests/*.rs`, so the
/// crate the test left never named it and has nothing to stop naming.
///
/// The header pass is where this differs from a module move in substance rather than shape. A test
/// reaches its subject through whatever path compiled at the time it was written, which in this
/// workspace may be **two** re-export facades deep: `tddy_daemon::host_registry` is
/// `tddy-session-lifecycle`'s re-export of `tddy-host-service`'s module. Re-pointing it one hop
/// short produces a test that compiles and still names the wrong crate, so every path is resolved
/// with [`defining_crate`].
///
/// # Errors
///
/// Refuses when the anchor is not a test binary, when the destination is not a crate, or when a
/// path the moved test names cannot be resolved to a defining crate.
pub fn resolve_test_binary_move(
    _workspace: &Workspace<'_>,
    _moving: &TestBinaryMove,
) -> Result<WorkspaceEdit> {
    // TODO(test-homes): implement
    todo!("resolve_test_binary_move: rename, re-point the header, extend [dev-dependencies]")
}

