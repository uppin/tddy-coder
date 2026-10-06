//! What `resolve` says until the operation itself is implemented, so nothing believes a retarget
//! happened.
//!
//! TODO(retarget-impl): delete this once `resolve` retargets the block; it exists only so the
//! published operation refuses rather than silently doing nothing.

use crate::RestructureError;

/// The refusal `resolve` raises: the backend does not support the operation yet.
pub(super) fn refusal() -> RestructureError {
    RestructureError::UnsupportedOp {
        backend: "Rust (node `retarget-impl` has not implemented it yet, TODO(retarget-impl))"
            .to_string(),
        op: "RetargetImpl".to_string(),
    }
}
