//! The refusals specific to `retarget_impl`, and to the field only it carries: `to_type`.
//!
//! All of them read the plan line alone, so they are made before any server is spawned and reported
//! by a plain `check`, `check --deep` and `apply` alike. Whatever needs the code (the members, the
//! new type's fields, the file's bindings) is `backends/rust/retarget_impl`'s.

use super::super::malformed;
use super::super::rust_syntax;
use super::super::{Anchor, ItemSegment, RefactorKind, RefactorOp};
use crate::Result;

/// `to_type` belongs to `retarget_impl` alone, and `retarget_impl` needs it, anchors by item, is an
/// inherent impl's and carries no field it has no use for.
pub(super) fn refuse_a_retarget_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::RetargetImpl {
        return match op.to_type {
            Some(_) => Err(malformed(format!(
                "`to_type` names the type an `impl` retargets to, which only `retarget_impl` \
                 honours — `{:?}` cannot",
                op.op
            ))),
            None => Ok(()),
        };
    }
    let Some(to_type) = op.to_type.as_deref() else {
        return Err(malformed(
            "`retarget_impl` needs `to_type`: the type the members move to",
        ));
    };
    one_path_type(to_type)?;
    anchored_on_an_inherent_impl_by_item(op)?;
    refuse_a_field_it_does_not_define(op)
}

/// `to_type` as exactly one path type: `app::roster::Roster`, generic arguments allowed on the last
/// segment.
fn one_path_type(text: &str) -> Result<()> {
    let refused = || {
        malformed(format!(
            "`to_type` must be one path type, `app::roster::Roster` with optional generic \
             arguments; `{text}` is not"
        ))
    };
    match rust_syntax::one_type(text) {
        Ok(syn::Type::Path(path)) if path.qself.is_none() => Ok(()),
        _ => Err(refused()),
    }
}

/// An `items` anchor, or an `item` anchor with no relative range, and no trait impl on the way.
fn anchored_on_an_inherent_impl_by_item(op: &RefactorOp) -> Result<()> {
    let paths = match &op.anchor {
        Anchor::Items { items, .. } => items.iter().collect::<Vec<_>>(),
        Anchor::Item {
            item,
            start: None,
            end: None,
            ..
        } => vec![item],
        _ => {
            return Err(malformed(
                "`retarget_impl` anchors by item (`items`, or a single `item`): a range or a \
                 symbol names no `impl` or member to retarget",
            ))
        }
    };
    for path in paths {
        if path
            .segments()
            .iter()
            .any(|segment| matches!(segment, ItemSegment::TraitImpl { .. }))
        {
            return Err(malformed(format!(
                "`retarget_impl` moves members of an inherent `impl`; `{path}` is a trait impl, \
                 and its members cannot change type without breaking the trait's contract"
            )));
        }
    }
    Ok(())
}

/// The new type is named by `to_type`: `to` and `name` would be read as something else, and a
/// `variant` is defined for the delegator only.
fn refuse_a_field_it_does_not_define(op: &RefactorOp) -> Result<()> {
    let field = if op.to.is_some() {
        "to"
    } else if op.name.is_some() {
        "name"
    } else if op.with_private_deps {
        "with_private_deps"
    } else if op
        .variant
        .as_deref()
        .is_some_and(|variant| variant != LEAVE_DELEGATOR)
    {
        "variant"
    } else {
        return Ok(());
    };
    Err(malformed(format!(
        "`{field}` is not a field of `retarget_impl`: the new type is named by `to_type`"
    )))
}

/// The one `variant` `retarget_impl` defines.
const LEAVE_DELEGATOR: &str = "leave_delegator";
