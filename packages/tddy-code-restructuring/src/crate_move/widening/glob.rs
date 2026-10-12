//! Rule 2: a module its parent re-exports by glob has every non-private item reached, whether or not
//! a path names it — a trait imported only so its methods can be called is reached by no reference.

use super::Widened;
use crate::crate_move::{ModuleHome, Result, Survey};
use crate::registry::Workspace;

/// The parent's top-level `use <module>::*;` of `home`, as `<file>:<line>: <statement>`, when the
/// file that declares the module holds one at any visibility.
///
/// # Errors
///
/// Refuses when the declaring file cannot be read.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): called by `surveyed` in the green phase"
)]
pub(crate) fn reexport_of(workspace: &Workspace<'_>, home: &ModuleHome) -> Result<Option<String>> {
    // TODO(reshape-move-widen): implement
    let _ = (workspace, home);
    todo!("the parent's glob re-export of the module")
}

/// Every top-level item of the surveyed module not written private, when the survey names a parent
/// glob, each with the reason `` through `<file>`'s glob ``.
///
/// # Errors
///
/// Refuses when the moving file cannot be read.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): called by `widened` in the green phase"
)]
pub(crate) fn glob_visible(workspace: &Workspace<'_>, survey: &Survey) -> Result<Vec<Widened>> {
    // TODO(reshape-move-widen): implement
    let _ = (workspace, survey);
    todo!("the items a parent glob makes visible")
}
