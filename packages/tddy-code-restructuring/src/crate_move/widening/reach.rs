//! Rules 1 and 4: the declarations a reference from outside the moving files reaches, and the ones
//! never touched.

use super::Widened;
use crate::crate_move::{MovingCluster, Result, Survey};
use crate::registry::Workspace;

/// Every `Item`, `Field` and `InherentMember` declaration of a moving file, not already `pub`, that
/// a file the move does not carry and that is not in the destination crate references — and each
/// inline `mod` on a reached item's path. `NoVisibility` declarations are never returned.
///
/// # Errors
///
/// Refuses when a moving file cannot be read.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): called by `widened` in the green phase"
)]
pub(crate) fn reached(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
    surveys: &[Survey],
) -> Result<Vec<Widened>> {
    // TODO(reshape-move-widen): implement
    let _ = (workspace, cluster, surveys);
    todo!("the declarations an outside reference reaches")
}
