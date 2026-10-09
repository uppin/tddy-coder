//! The refusals specific to `read_fields_through`, read from the plan line alone (RP1-RP4).
//!
//! They are made before any server is spawned and reported by a plain `check`, `check --deep` and
//! `apply` alike. Whatever needs the code (the statement boundary, the `self` sites, the binding a
//! new `let` would shadow, the state value's fields) is `backends/rust/read_fields_through`'s.

use super::super::malformed;
use super::super::{RefactorKind, RefactorOp};
use crate::Result;

/// `read_fields_through` needs `name` (one identifier other than `self`) and `expr`, anchors on a
/// range, and carries no field it has no use for.
pub(super) fn refuse_a_rebind_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::ReadFieldsThrough {
        return Ok(());
    }
    // TODO(reshape-methods-leave-type): implement RP1-RP4; until then the operation is refused
    // rather than read as honoured.
    Err(malformed(
        "`read_fields_through` is not implemented yet (TODO(reshape-methods-leave-type))",
    ))
}
