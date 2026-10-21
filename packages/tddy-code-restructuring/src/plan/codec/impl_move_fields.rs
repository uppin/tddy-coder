//! The refusals specific to `move_impl_members`, read from the plan line alone.
//!
//! A member move needs `to` (with `name`, the parent of the module it creates) and an anchor on the
//! members of one inherent `impl`, by item. Everything else a line can carry — `reexport` (no path
//! names a member, so there is nothing to re-export), `canonical_paths`, `to_type`, `also`,
//! `to_file`, `variant`, `expr`, `callee`, `type`, `order`, `with_private_deps` — is refused naming
//! the field, before any server is spawned. A `<Type>` block anchor is `move_item`'s, and a member
//! of a trait `impl` (`<T as Trait>::m`) cannot move without the rest of its block.
//!
//! Whatever needs the code (the run, the destination's blocks, the bindings) is read by the Rust
//! backend's `impl_move` module.

use super::super::malformed;
use super::super::{Anchor, ItemPath, ItemSegment, RefactorKind, RefactorOp};
use crate::Result;

/// Refuse a `move_impl_members` line that misses what it needs or carries what it cannot honour;
/// every other operation passes untouched.
pub(super) fn rules(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::MoveImplMembers {
        return Ok(());
    }
    if op.to.is_none() {
        return Err(malformed(
            "`move_impl_members` needs `to`: the module the members move into, rooted at the \
             package name like an item path",
        ));
    }
    for path in member_paths(&op.anchor)? {
        refuse_a_path_it_cannot_move(path)?;
    }
    refuse_a_field_it_cannot_honour(op)
}

/// The item paths the anchor names. A range or a symbol names no member, so it is refused.
fn member_paths(anchor: &Anchor) -> Result<Vec<&ItemPath>> {
    match anchor {
        Anchor::Item { item, .. } => Ok(vec![item]),
        Anchor::Items { items, .. } => Ok(items.iter().collect()),
        Anchor::Range { .. } => Err(anchored_by_item("a range names no member to move")),
        Anchor::Symbol { .. } => Err(anchored_by_item("a symbol names no member to move")),
    }
}

fn anchored_by_item(reason: &str) -> crate::RestructureError {
    malformed(format!(
        "`move_impl_members` anchors by item (`items`, or a single `item`): {reason}"
    ))
}

/// A path naming a whole block (`<Host>`) or a member of a trait `impl` (`<Host as Display>::fmt`)
/// is refused; a plain member path is the operation's own.
fn refuse_a_path_it_cannot_move(path: &ItemPath) -> Result<()> {
    let segments = path.segments();
    if let Some(block) = segments
        .iter()
        .find(|segment| matches!(segment, ItemSegment::InherentImpl { .. }))
    {
        return Err(malformed(format!(
            "`move_impl_members` cannot move `{}`: a whole `impl` block moves with `move_item`",
            block.spelled()
        )));
    }
    if let Some(at) = segments
        .iter()
        .position(|segment| matches!(segment, ItemSegment::TraitImpl { .. }))
    {
        let member = segments[at..]
            .iter()
            .map(ItemSegment::spelled)
            .collect::<Vec<_>>()
            .join("::");
        return Err(malformed(format!(
            "`move_impl_members` cannot move `{member}`: a trait `impl` moves whole with `move_item`"
        )));
    }
    Ok(())
}

/// The fields a member move has no use for. `name` (the module it creates) and `to` are its own.
fn refuse_a_field_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    let refused = [
        ("reexport", op.reexport.is_some()),
        ("canonical_paths", op.canonical_paths),
        ("to_type", op.to_type.is_some()),
        ("to_file", op.to_file),
        ("also", !op.also.is_empty()),
        ("callee", op.callee.is_some()),
        ("variant", op.variant.is_some()),
        ("expr", op.expr.is_some()),
        ("type", op.type_.is_some()),
        ("order", !op.order.is_empty()),
        ("with_private_deps", op.with_private_deps),
    ]
    .into_iter()
    .find_map(|(field, present)| present.then_some(field));
    match refused {
        Some(field) => Err(malformed(format!(
            "`move_impl_members` cannot honour `{field}`: a member move names its members and \
             `to`, and writes no facade and re-points no caller"
        ))),
        None => Ok(()),
    }
}
