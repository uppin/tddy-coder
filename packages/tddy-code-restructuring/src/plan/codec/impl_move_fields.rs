//! The refusals specific to `move_impl_members`, read from the plan line alone.
//!
//! A member move needs `to` (with `name`, the parent of the module it creates) and an anchor on the
//! members of one inherent `impl`, by item. Everything else a line can carry — `reexport` (no path
//! names a member, so there is nothing to re-export), `canonical_paths`, `to_type`, `also`,
//! `to_file`, `variant`, `expr`, `callee`, `type`, `order`, `with_private_deps` — is refused naming
//! the field, before any server is spawned. A `<Type>` block anchor is `move_item`'s, and a member
//! of a trait `impl` (`<T as Trait>::m`) cannot move without the rest of its block.
//!
//! Whatever needs the code (the run, the destination's blocks, the bindings) is
//! `backends/rust/impl_move`'s.

use super::super::{RefactorKind, RefactorOp};
use crate::Result;

/// Refuse a `move_impl_members` line that misses what it needs or carries what it cannot honour;
/// every other operation passes untouched.
pub(super) fn rules(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::MoveImplMembers {
        return Ok(());
    }
    // TODO(reshape-move-impl-members): implement P1 and P6–P9 (changeset § Plan-line schema).
    Err(crate::RestructureError::UnsupportedOp {
        backend: "`move_impl_members` is not implemented yet (#reshape 13/19)".to_string(),
        op: "MoveImplMembers".to_string(),
    })
}
