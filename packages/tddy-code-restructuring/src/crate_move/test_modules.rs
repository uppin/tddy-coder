//! The test modules a cross-crate move takes along from **beside** the modules it moves.
//!
//! A `#[cfg(test)] mod t;` declared in the file that declares a moved module, when that file stays
//! in the origin, is a sibling of the moved code. Its `super::` paths name that code, so once the
//! code leaves, a test module left behind no longer compiles — `#carve` 21/21 R9 moved three such
//! files by hand. This module decides which of them **follow** the move (everything they name in the
//! origin is moving, and they name at least one moving thing), writes the edits that take them
//! along, and says which stay and why (`#reshape` 13/24).
//!
//! A test declaration inside a file that moves is a directory child, and [`super::carried`] carries
//! it; this module only tells the header pass that such a file is test code throughout
//! ([`test_gated`]).

use std::collections::BTreeSet;

use crate::edit::FileEdit;
use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::module_files::MovedFile;
use super::source_scan::{test_declarations, TestDeclaration};
use super::{ModuleReferences, Move, Result};

/// A test module that follows the move.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` takes the followers along"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FollowingTest {
    /// The file that declares it, which stays in the origin.
    pub(crate) declared_in: String,
    /// Its declaration there, with the attributes and doc comments that leave with it.
    pub(crate) declaration: TestDeclaration,
    /// Its module path in the origin, outermost first: the declaring file's path, then its name.
    pub(crate) module_path: Vec<String>,
    /// Every file it spans, its own first, each with where it lands in the destination.
    pub(crate) files: Vec<MovedFile>,
    /// The moved module it lands beside: the first member, in plan order, that it names.
    pub(crate) follows: String,
}

/// What the move decided about the test modules beside its members.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` reads it"
)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TestModules {
    pub(crate) following: Vec<FollowingTest>,
    /// One per follower, and one per staying test module that names something the move takes,
    /// naming the path that keeps it.
    pub(crate) notes: Vec<String>,
}

/// Sort every `#[cfg(test)] mod t;` declared beside a member into those that follow and those that
/// stay, from the text of the tree and the origin's re-exports alone, so a plain `check` reaches the
/// same verdict `apply` does.
///
/// `earlier` holds the module paths operations before this one already moved, which count as
/// moving.
///
/// # Errors
///
/// Refuses when a file the survey must read cannot be, or a path climbs above the crate root.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` and `unfollowable` call it"
)]
pub(crate) fn sorted(
    workspace: &Workspace<'_>,
    members: &[Move],
    earlier: &BTreeSet<String>,
) -> Result<TestModules> {
    // TODO(reshape-tests-follow): implement
    let _ = (workspace, members, earlier);
    todo!("test_modules::sorted")
}

/// `modules`, with every follower whose items a file outside `travelling` names moved back to the
/// staying side, with a note naming that file: taking it along would break that file.
///
/// # Errors
///
/// Refuses for every reason the reference engine does.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` asks the engine"
)]
pub(crate) fn kept_by_outside_references(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    modules: TestModules,
    travelling: &BTreeSet<String>,
) -> Result<TestModules> {
    // TODO(reshape-tests-follow): implement
    let _ = (engine, workspace, modules, travelling);
    todo!("test_modules::kept_by_outside_references")
}

/// Every file that travels: each member's own file, the files it carries, and every file of each
/// following test module — what the caller survey must not count as a caller.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` builds its travelling set here"
)]
pub(crate) fn travelling(
    members: &[Move],
    carried: &[MovedFile],
    following: &[FollowingTest],
) -> BTreeSet<String> {
    // TODO(reshape-tests-follow): implement
    let _ = (members, carried, following);
    todo!("test_modules::travelling")
}

/// The edits that take the followers along — a rename per file, its re-pointed header (read as test
/// code throughout), the declaration removed from the origin and appended to the destination root
/// under `#[cfg(test)]` — and the crates they name, every one of them a `[dev-dependencies]` entry.
///
/// # Errors
///
/// Refuses for every reason the header pass does.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `cluster_edits` absorbs the edits"
)]
pub(crate) fn follow_changes(
    workspace: &Workspace<'_>,
    members: &[Move],
    following: &[FollowingTest],
    co_moving: &BTreeSet<String>,
) -> Result<(Vec<FileEdit>, BTreeSet<String>)> {
    // TODO(reshape-tests-follow): implement
    let _ = (workspace, members, following, co_moving);
    todo!("test_modules::follow_changes")
}

/// The carried files reached through a `#[cfg(test)] mod` declaration, or below one: the files the
/// header pass reads as test code throughout.
///
/// # Errors
///
/// Refuses when a declaring file cannot be read.
pub(crate) fn test_gated(
    workspace: &Workspace<'_>,
    carried: &[MovedFile],
) -> Result<BTreeSet<String>> {
    let mut gated = BTreeSet::new();
    // A member's files begin with its own, which has no `below`, so each run of files that starts
    // at one is one member's tree, and a declaring file is looked up within the run.
    for tree in carried.chunk_by(|_, next| !next.below.is_empty()) {
        for file in tree.iter().filter(|file| !file.below.is_empty()) {
            if declared_under_test(workspace, tree, &file.below)? {
                gated.insert(file.from.clone());
            }
        }
    }
    Ok(gated)
}

/// Whether one of the modules `below` leads through is declared `#[cfg(test)]` in the file that
/// declares it, among the files of its member's `tree`.
///
/// TODO(reshape-tests-follow): only a declaration at the top level of its declaring file is read;
/// a test declaration inside an inline module on the way down is not.
fn declared_under_test(
    workspace: &Workspace<'_>,
    tree: &[MovedFile],
    below: &[String],
) -> Result<bool> {
    for (depth, name) in below.iter().enumerate() {
        let Some(declaring) = tree.iter().find(|file| file.below == below[..depth]) else {
            continue;
        };
        let text = workspace.read(&declaring.from)?;
        if test_declarations(&text)
            .iter()
            .any(|declaration| &declaration.name == name)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every reason operation `index` of `ops` (`op`) cannot take a following test module along, read
/// statically: its landing file already exists in the destination, or the destination root already
/// declares its name — a merge.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `preconditions::unrunnable` reports them"
)]
pub(crate) fn unfollowable(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
    op: &RefactorOp,
) -> Result<Vec<String>> {
    // TODO(reshape-tests-follow): implement
    let _ = (workspace, ops, index, op);
    todo!("test_modules::unfollowable")
}
