use super::super::malformed;

use crate::{Anchor, RefactorKind, Result};

use super::super::RefactorOp;

/// The refusals specific to the signature and call-site operations, and to the fields only they
/// carry: a field an operation cannot honour is refused rather than ignored, and a field it needs
/// is refused when absent, before any server is spawned.
pub(crate) fn refuse_a_signature_operation_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    refuse_a_field_the_operation_cannot_honour(op)?;
    refuse_a_missing_or_unknown_field(op)?;
    refuse_a_call_site_anchored_off_a_call(op)
}

/// `type`, `expr` and `order` each belong to a few operations only.
fn refuse_a_field_the_operation_cannot_honour(op: &RefactorOp) -> Result<()> {
    use RefactorKind::*;

    let kind = op.op;
    if op.type_.is_some() && !matches!(kind, ChangeParamType | AddParam | ChangeReturnType) {
        return Err(malformed(format!(
            "`type` names a Rust type, which only `change_param_type`, `add_param` and \
             `change_return_type` honour — `{kind:?}` cannot"
        )));
    }
    if op.expr.is_some()
        && !matches!(
            kind,
            AddCallArg | ChangeCallArg | RetargetImpl | ReadFieldsThrough
        )
    {
        return Err(malformed(format!(
            "`expr` names a call argument, which only `add_call_arg` and `change_call_arg` \
             honour, the receiver of `retarget_impl`'s delegator, or the value \
             `read_fields_through` binds — `{kind:?}` cannot"
        )));
    }
    if !op.order.is_empty() && !matches!(kind, ReorderParams | ReorderCallArgs) {
        return Err(malformed(format!(
            "`order` names a new order, which only `reorder_params` and `reorder_call_args` \
             honour — `{kind:?}` cannot"
        )));
    }
    Ok(())
}

/// The fields a signature or call-site operation needs, and the positions its `variant` may name.
fn refuse_a_missing_or_unknown_field(op: &RefactorOp) -> Result<()> {
    use RefactorKind::*;

    let kind = op.op;
    let needs = |what: &str, present: bool| -> Result<()> {
        if present {
            Ok(())
        } else {
            Err(malformed(format!("`{kind:?}` needs `{what}`")))
        }
    };
    let position = op.variant.as_deref();
    match kind {
        ChangeParamType => {
            needs("name", op.name.is_some())?;
            needs("type", op.type_.is_some())
        }
        AddParam => {
            needs("name", op.name.is_some())?;
            needs("type", op.type_.is_some())?;
            needs("variant", position.is_some())?;
            match position.filter(|variant| !a_parameter_position(variant)) {
                Some(variant) => Err(malformed(format!(
                    "`add_param`'s `variant` is `first`, `last` or `after:<parameter>`, and \
                     `{variant}` is none of them"
                ))),
                None => Ok(()),
            }
        }
        ReorderParams | ReorderCallArgs => needs("order", !op.order.is_empty()),
        ChangeReturnType => match (op.type_.is_some(), position) {
            (true, None) | (false, Some("wrap_result" | "wrap_option" | "unwrap")) => Ok(()),
            (false, Some(variant)) => Err(malformed(format!(
                "`change_return_type`'s `variant` is `wrap_result`, `wrap_option` or \
                 `unwrap`, and `{variant}` is none of them"
            ))),
            _ => Err(malformed(
                "`change_return_type` needs exactly one of `type` and `variant`",
            )),
        },
        AddCallArg | RemoveCallArg | ChangeCallArg => {
            needs("variant", position.is_some())?;
            if let Some(variant) = position.filter(|variant| !an_argument_position(variant)) {
                return Err(malformed(format!(
                    "`{kind:?}`'s `variant` is `first`, `last` or a one-based position, and \
                     `{variant}` is none of them"
                )));
            }
            if matches!(kind, AddCallArg | ChangeCallArg) {
                needs("expr", op.expr.is_some())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A call-site operation edits one call, so its anchor is an item with a relative range over it.
fn refuse_a_call_site_anchored_off_a_call(op: &RefactorOp) -> Result<()> {
    let over_a_range = matches!(
        op.anchor,
        Anchor::Item {
            start: Some(_),
            end: Some(_),
            ..
        }
    );
    if op.op.edits_a_call_site() && !over_a_range {
        return Err(malformed(format!(
            "`{:?}` is anchored on one call expression: an item anchor with a relative range \
             over the call",
            op.op
        )));
    }
    Ok(())
}

/// `first`, `last` or `after:<parameter>` — where `add_param` puts the parameter.
fn a_parameter_position(variant: &str) -> bool {
    matches!(variant, "first" | "last")
        || variant
            .strip_prefix("after:")
            .is_some_and(|name| !name.is_empty())
}

/// `first`, `last` or a one-based position — which argument a call-site operation addresses.
fn an_argument_position(variant: &str) -> bool {
    matches!(variant, "first" | "last") || variant.parse::<u32>().is_ok_and(|index| index >= 1)
}
