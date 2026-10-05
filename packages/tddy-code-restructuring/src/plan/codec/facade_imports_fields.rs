//! The refusals specific to `repoint_facade_imports`: a line that carries only an anchor.
//!
//! Every other field of the plan line is one the operation cannot honour, and refusing it is
//! decided from the line alone, before any server is spawned, so plain `check`, `check --deep` and
//! `apply` report it alike. What needs the code (which paths go through a facade, whether the
//! defining crate is declared) is `backends/rust/repoint_facade`'s.
//!
//! TODO(repoint-facade): implement the refusals. Until then nothing is refused here, and an
//! operation that gets past the codec is refused by `resolve` naming the node, so no plan line
//! succeeds on the strength of a field this operation would ignore.

use super::super::RefactorOp;
use crate::Result;

/// A `repoint_facade_imports` line carries an anchor and nothing else.
///
/// TODO(repoint-facade): refuse `to`, `name`, `reexport`, `variant`, `type`, `expr`, `order`,
/// `also`, `to_file`, `with_private_deps`, `callee` and `canonical_paths`, naming the field, and
/// refuse a `range` anchor.
pub(super) fn refuse_a_facade_repoint_it_cannot_honour(_op: &RefactorOp) -> Result<()> {
    Ok(())
}
