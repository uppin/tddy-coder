//! What a cross-crate move has to widen so the code it leaves behind still compiles.
//!
//! Across a crate boundary there is no visibility narrower than `pub`, so "as little as needed" is a
//! question of **which** declarations are widened, never of how far: a declaration of a moving file
//! is widened when a file the move does not carry, outside the destination crate, still reaches it
//! (`reach`); when the module's parent re-exports it by glob and it is not private (`glob`); or when
//! the signature of a declaration widened for either reason names it (`escaping`). Each edit is
//! addressed by the position the engine reported for the declaration (`declaration`), never by its
//! name, and each is reported as a [`VisibilityChange`].

use super::{MovingCluster, Result, Survey};
use crate::edit::{Position, VisibilityChange, WorkspaceEdit};
use crate::registry::Workspace;

pub(crate) mod declaration;
pub(crate) mod escaping;
pub(crate) mod glob;
pub(crate) mod reach;

/// One declaration the move widens to `pub`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): read by `widened` in the green phase"
)]
pub(crate) struct Widened {
    /// The moving file that declares it, relative to the repository root.
    pub(crate) file: String,
    /// Its name as the report states it: `AgentRoster`, `AgentRoster::broadcast`, `inner::Probe`.
    pub(crate) name: String,
    /// Where its name is written in `file`, one-based, in characters.
    pub(crate) declared_at: Position,
    /// Why it is widened when no reference of its own says so: through a parent's glob, or named by
    /// a widened signature. `None` when an outside reference reaches it.
    pub(crate) reason: Option<String>,
}

/// `edit`, with every widening the move needs merged into the change of the moving file it lands
/// in, and the report of them in moving-file and then source order.
///
/// # Errors
///
/// Refuses a reached declaration whose visibility keyword is on a line above its name.
pub(crate) fn widened(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
    edit: WorkspaceEdit,
    surveys: &[Survey],
) -> Result<(WorkspaceEdit, Vec<VisibilityChange>)> {
    // TODO(reshape-move-widen): implement — `reach`, `glob` and `escaping` decide, `declaration`
    // edits, and the edits merge into each moving file's existing change. Until then the move
    // widens nothing, which is the behaviour it has always had.
    let _ = (workspace, cluster, surveys);
    Ok((edit, Vec::new()))
}
