use super::super::malformed;

use crate::{RefactorKind, Result};

use super::super::RefactorOp;

/// `canonical_paths` rewrites the paths of a moved item's text, which only `move_item` has: any
/// other operation is refused rather than having the field ignored, before any server is spawned.
pub(super) fn refuse_canonical_paths_outside_move_item(op: &RefactorOp) -> Result<()> {
    if op.canonical_paths && op.op != RefactorKind::MoveItem {
        return Err(malformed(format!(
            "`canonical_paths` spells the paths of a moved item's text as the paths that define \
             them, which only `move_item` honours — `{:?}` cannot",
            op.op
        )));
    }
    Ok(())
}
