//! What a plan may say about a crate a move creates: `name` on `move_module_to_crate` and
//! `move_cluster_to_crate` is the new crate's `[package] name`.
//!
//! Checked over the whole plan rather than one line, because one of the refusals is about two
//! operations: a plan that creates the same crate twice.

use crate::plan::RefactorOp;
use crate::Result;

/// Refuse what no crate move can honour about creating a crate: `name` on
/// `move_test_binary_to_crate`, a `name` that is not a Cargo package name, and two operations that
/// create a crate at the same `to`.
#[allow(
    dead_code,
    reason = "TODO(reshape-new-crate): implement — `parse_ops` calls it"
)]
pub(super) fn refuse_crate_creations_it_cannot_honour(ops: &[RefactorOp]) -> Result<()> {
    // TODO(reshape-new-crate): implement
    let _ = ops;
    todo!("crate_move_fields::refuse_crate_creations_it_cannot_honour")
}
