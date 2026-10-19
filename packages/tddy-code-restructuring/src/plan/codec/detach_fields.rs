//! The refusals specific to `detach_method`, read from the plan line alone (R5's plain-`check`
//! half).
//!
//! They are made before any server is spawned and reported by a plain `check`, `check --deep` and
//! `apply` alike: a missing or malformed `name` (one identifier, not `self`, not a keyword), an
//! anchor that names no members of one inherent `impl` (anything but `items` or a single `item`),
//! and every field the operation does not define, by name. Whatever needs the code (the members,
//! their receivers, the callers) is `backends/rust/detach_method`'s.

use super::super::{RefactorKind, RefactorOp};
use crate::Result;

/// Refuse a `detach_method` line that misses what it needs or carries what it cannot honour; every
/// other operation passes untouched.
pub(super) fn rules(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::DetachMethod {
        return Ok(());
    }
    // TODO(reshape-detach-method): implement
    todo!("TODO(reshape-detach-method): the plan-line refusals of `detach_method`")
}
