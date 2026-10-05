//! The refusals of `repoint_facade_imports` that need the code: an undeclared defining crate, a
//! rename in a body, a path spelled across whitespace, a duplicate binding, an attribute above a
//! group that must split.
//!
//! TODO(repoint-facade): the refusals themselves; only the refusal of an unimplemented operation
//! exists.

use crate::RestructureError;

/// The refusal an unimplemented step raises: the backend does not support the operation yet.
pub(super) fn unfinished() -> RestructureError {
    RestructureError::UnsupportedOp {
        backend: "Rust (node `repoint-facade` has not implemented it yet, TODO(repoint-facade))"
            .to_string(),
        op: "RepointFacadeImports".to_string(),
    }
}
