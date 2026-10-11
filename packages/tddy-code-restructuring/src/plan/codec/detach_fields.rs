//! The refusals specific to `detach_method`, read from the plan line alone (DP1-DP3).
//!
//! They are made before any server is spawned and reported by a plain `check`, `check --deep` and
//! `apply` alike. Whatever needs the code (the members, their receivers, the callers) is
//! `backends/rust/detach_method`'s.

use super::super::malformed;
use super::super::{RefactorKind, RefactorOp};
use crate::Result;

/// `detach_method` needs `name` (one identifier other than `self`), anchors on members of one
/// inherent `impl` by item, and carries no field it has no use for.
pub(super) fn refuse_a_detach_it_cannot_honour(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::DetachMethod {
        return Ok(());
    }
    // TODO(reshape-backend-session): implement DP1-DP3; until then the operation is refused rather
    // than read as honoured.
    Err(malformed(
        "`detach_method` is not implemented yet (TODO(reshape-backend-session))",
    ))
}
