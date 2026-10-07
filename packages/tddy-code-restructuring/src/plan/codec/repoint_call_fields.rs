//! The refusals specific to `repoint_call`, and to the field only it carries: `callee`.
//!
//! All of them read the plan line alone, so they are made before any server is spawned and reported
//! by a plain `check`, `check --deep` and `apply` alike. Whatever needs the code (that the range is
//! one call, that the old callee holds no call, which references a method has) is
//! `backends/rust/repoint_call`'s.
//!
//! The form is read from the anchor's shape: an item anchor with a relative range (or a v1 range
//! anchor) over one call is the **single** form; an item anchor with neither `start` nor `end` is
//! the **bulk** form, whose `callee` is a `$receiver<hops>.<method>` template.

use super::super::malformed;
use super::super::rust_syntax;
use super::super::{Anchor, ItemSegment, RefactorKind, RefactorOp};
use crate::Result;

/// `callee` belongs to `repoint_call` alone, and `repoint_call` needs it, anchors on one call or on
/// one method, and carries no field it has no use for.
pub(super) fn refuse_a_repoint_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::RepointCall {
        return match op.callee {
            Some(_) => Err(malformed(format!(
                "`callee` names the callee a call is re-pointed to, which only `repoint_call` \
                 honours — `{:?}` cannot",
                op.op
            ))),
            None => Ok(()),
        };
    }
    refuse_a_field_it_cannot_honour(op)?;
    let Some(callee) = op.callee.as_deref() else {
        return Err(malformed(
            "`repoint_call` needs `callee`: the complete new callee, or in the bulk form a \
             `$receiver<hops>.<method>` template",
        ));
    };
    match form_of(&op.anchor)? {
        Form::Single => single_callee(callee),
        Form::Bulk(method) => rust_syntax::one_receiver_template(callee, &method).map(|_| ()),
    }
}

/// Which of the two forms an anchor asks for.
enum Form {
    Single,
    /// The method whose references are re-pointed.
    Bulk(String),
}

fn form_of(anchor: &Anchor) -> Result<Form> {
    match anchor {
        Anchor::Range { .. }
        | Anchor::Item {
            start: Some(_),
            end: Some(_),
            ..
        } => Ok(Form::Single),
        Anchor::Item {
            item,
            start: None,
            end: None,
            ..
        } => match item.segments().last() {
            Some(ItemSegment::Named(method)) => Ok(Form::Bulk(method.clone())),
            _ => Err(malformed(format!(
                "`repoint_call` in its bulk form anchors on a method, and `{item}` names none"
            ))),
        },
        _ => Err(malformed(
            "`repoint_call` anchors by item: the function holding one call with a relative \
             range over it, or a method with no range, whose every call is re-pointed",
        )),
    }
}

/// The new callee of one call: one chain, and no receiver placeholder.
fn single_callee(callee: &str) -> Result<()> {
    if callee.contains("$receiver") {
        return Err(malformed(format!(
            "`$receiver` is the placeholder of the bulk form (an item anchor with no range); the \
             single form names the whole callee, and `{callee}` does not"
        )));
    }
    rust_syntax::one_callee(callee).map(|_| ())
}

/// A field `repoint_call` has no use for is refused rather than ignored.
fn refuse_a_field_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    let field = if op.variant.is_some() {
        "variant"
    } else if op.name.is_some() {
        "name"
    } else if op.to.is_some() {
        "to"
    } else if op.with_private_deps {
        "with_private_deps"
    } else {
        return Ok(());
    };
    Err(malformed(format!(
        "`{field}` is a field `repoint_call` cannot honour: it edits the callee of a call and \
         names the new one in `callee`; arguments are edited by `add_call_arg` and its siblings"
    )))
}
