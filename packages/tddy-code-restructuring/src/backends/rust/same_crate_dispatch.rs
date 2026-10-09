//! The three moves within one crate, routed from one arm of `check` and of `resolve`.
//!
//! `move_item`, `reparent_module` and `move_impl_members` are each authored in this backend and
//! informed by the server; `rust.rs` names them once here rather than once per kind, so a new
//! same-crate move does not grow the two dispatchers on the function-size list.

use super::{impl_move, item_move, module_reparent, RustBackend};
use crate::edit::Resolution;
use crate::plan::{RefactorKind, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// Whether `kind` is one of the moves within one crate this module routes.
pub(super) fn handles(kind: RefactorKind) -> bool {
    matches!(
        kind,
        RefactorKind::MoveItem | RefactorKind::ReparentModule | RefactorKind::MoveImplMembers
    )
}

/// What a static check finds wrong with a same-crate move, from the text alone.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    match op.op {
        RefactorKind::ReparentModule => module_reparent::findings(op, workspace),
        RefactorKind::MoveImplMembers => impl_move::findings(op, workspace),
        _ => item_move::findings(op, workspace),
    }
}

impl RustBackend {
    /// Resolve a same-crate move into the edits to every file it changes.
    pub(super) fn same_crate_move(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        match op.op {
            RefactorKind::ReparentModule => self.reparent_module(op, workspace),
            RefactorKind::MoveImplMembers => self.move_impl_members(op, workspace),
            _ => self.move_items(op, workspace),
        }
    }
}
