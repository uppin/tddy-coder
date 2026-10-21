//! The refusals specific to `detach_method`, read from the plan line alone (R5's plain-`check`
//! half).
//!
//! They are made before any server is spawned and reported by a plain `check`, `check --deep` and
//! `apply` alike: a missing or malformed `name` (one identifier, not `self`, not a keyword), an
//! anchor that names no members of one inherent `impl` (anything but `items` or a single `item`),
//! and every field the operation does not define, by name. Whatever needs the code (the members,
//! their receivers, the callers) is `backends/rust/detach_method`'s.

use super::super::rust_syntax;
use super::super::{malformed, Anchor, RefactorKind, RefactorOp};
use crate::Result;

/// Refuse a `detach_method` line that misses what it needs or carries what it cannot honour; every
/// other operation passes untouched.
pub(super) fn rules(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::DetachMethod {
        return Ok(());
    }
    refuse_a_field_it_does_not_define(op)?;
    anchored_on_members(op)?;
    refuse_a_name_it_cannot_take(op)
}

/// The members are named by the anchor and the parameter by `name`: every other field a line can
/// carry is one `detach_method` has no use for, and is refused by name rather than ignored.
fn refuse_a_field_it_does_not_define(op: &RefactorOp) -> Result<()> {
    let carried = [
        ("to", op.to.is_some()),
        ("variant", op.variant.is_some()),
        ("with_private_deps", op.with_private_deps),
        ("reexport", op.reexport.is_some()),
        ("to_file", op.to_file),
        ("also", !op.also.is_empty()),
        ("canonical_paths", op.canonical_paths),
        ("to_type", op.to_type.is_some()),
        ("callee", op.callee.is_some()),
        ("type", op.type_.is_some()),
        ("expr", op.expr.is_some()),
        ("order", !op.order.is_empty()),
    ];
    match carried.iter().find(|(_, carried)| *carried) {
        Some((field, _)) => Err(malformed(format!(
            "`{field}` is not a field of `detach_method`: the members are named by `anchor`, and \
             the parameter they become by `name`"
        ))),
        None => Ok(()),
    }
}

/// `items`, or a single `item` with no range: a range or a symbol names no member of an `impl`.
fn anchored_on_members(op: &RefactorOp) -> Result<()> {
    match &op.anchor {
        Anchor::Items { .. }
        | Anchor::Item {
            start: None,
            end: None,
            ..
        } => Ok(()),
        _ => Err(malformed(
            "`detach_method` anchors on members of one inherent `impl`: `items`, or a single \
             `item` with no range, names them; a range or a symbol names none",
        )),
    }
}

/// `name` is the parameter the receiver becomes: one identifier, and not `self`.
fn refuse_a_name_it_cannot_take(op: &RefactorOp) -> Result<()> {
    let Some(name) = op.name.as_deref() else {
        return Err(malformed(
            "`detach_method` needs `name`: the parameter the receiver becomes",
        ));
    };
    if name == "self" || !rust_syntax::is_identifier(name) {
        return Err(malformed(format!(
            "`name` must be one identifier other than `self`; `{name}` is not a name a parameter \
             can take"
        )));
    }
    Ok(())
}
