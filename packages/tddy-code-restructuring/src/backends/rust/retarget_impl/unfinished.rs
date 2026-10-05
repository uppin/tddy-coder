//! What an operation that is published but not yet implemented says, so it can never read as done.

use crate::RestructureError;

/// The one sentence both the static check and the refusal carry; it names the node that owns the
/// implementation.
pub(super) fn reason() -> String {
    "`retarget_impl` is published and not implemented yet (node `retarget-impl`, \
     TODO(retarget-impl): implement), so nothing was examined"
        .to_string()
}

/// The refusal `resolve` raises: the backend does not support the operation yet.
pub(super) fn refusal() -> RestructureError {
    RestructureError::UnsupportedOp {
        backend: "Rust (node `retarget-impl` has not implemented it yet, TODO(retarget-impl))"
            .to_string(),
        op: "RetargetImpl".to_string(),
    }
}
