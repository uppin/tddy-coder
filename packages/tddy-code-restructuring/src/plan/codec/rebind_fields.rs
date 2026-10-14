//! The refusals specific to `read_fields_through`, read from the plan line alone (RP1-RP4).
//!
//! They are made before any server is spawned and reported by a plain `check`, `check --deep` and
//! `apply` alike. Whatever needs the code (the statement boundary, the `self` sites, the binding a
//! new `let` would shadow, the state value's fields) is `backends/rust/read_fields_through`'s.

use super::super::malformed;
use super::super::{Anchor, RefactorKind, RefactorOp};
use crate::Result;

/// `read_fields_through` needs `name` (one identifier other than `self`) and `expr`, anchors on a
/// range, and carries no field it has no use for.
pub(super) fn refuse_a_rebind_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::ReadFieldsThrough {
        return Ok(());
    }
    anchored_on_a_range(&op.anchor)?;
    let name = one_binding(op)?;
    let Some(_) = op.expr.as_deref() else {
        return Err(malformed(format!(
            "`read_fields_through` needs `expr`: the value `{name}` is bound to"
        )));
    };
    refuse_a_field_it_does_not_define(op)
}

/// A `range`, or an `item` with a relative range (which a run lowers to one).
fn anchored_on_a_range(anchor: &Anchor) -> Result<()> {
    let kind = match anchor {
        Anchor::Range { .. }
        | Anchor::Item {
            start: Some(_),
            end: Some(_),
            ..
        } => return Ok(()),
        Anchor::Item { .. } => "item",
        Anchor::Symbol { .. } => "symbol",
        Anchor::Items { .. } => "items",
    };
    Err(malformed(format!(
        "`read_fields_through` anchors on a range (a `range`, or an `item` with a relative \
         range): `{kind}` names no statements to rebind"
    )))
}

/// The binding: one identifier, not `self` and not a keyword.
fn one_binding(op: &RefactorOp) -> Result<&str> {
    let Some(name) = op.name.as_deref() else {
        return Err(malformed(
            "`read_fields_through` needs `name`: the binding the range reads `self` through",
        ));
    };
    match syn::parse_str::<syn::Ident>(name) {
        Ok(ident) if ident == name && name != "self" => Ok(name),
        _ => Err(malformed(format!(
            "`name` must be one identifier other than `self`; `{name}` is not"
        ))),
    }
}

/// The fields the new binding is not described by: the range names the statements, `name` and
/// `expr` the binding.
fn refuse_a_field_it_does_not_define(op: &RefactorOp) -> Result<()> {
    let field = [
        ("to", op.to.is_some()),
        ("to_type", op.to_type.is_some()),
        ("variant", op.variant.is_some()),
        ("callee", op.callee.is_some()),
        ("type", op.type_.is_some()),
        ("order", !op.order.is_empty()),
        ("reexport", op.reexport.is_some()),
        ("also", !op.also.is_empty()),
        ("to_file", op.to_file),
        ("canonical_paths", op.canonical_paths),
        ("with_private_deps", op.with_private_deps),
    ]
    .into_iter()
    .find_map(|(field, present)| present.then_some(field));
    match field {
        Some(field) => Err(malformed(format!(
            "`{field}` is not a field of `read_fields_through`: the range names the statements, \
             `name` the binding and `expr` its value"
        ))),
        None => Ok(()),
    }
}
