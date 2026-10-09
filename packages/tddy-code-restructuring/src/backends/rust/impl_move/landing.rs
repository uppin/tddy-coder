//! Where moved members land in the destination: the one block of the same header they join, or a
//! new block.
//!
//! Rule R3: the destination's scope (inline modules included) is read for inherent `impl` blocks
//! whose header — generics, self type, `where` clause and attached attributes — is token-equal to
//! the origin's. Exactly one such block is joined; none, or more than one, opens a new block.

use std::ops::Range;

/// Where the members go in the destination's text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): constructed by `block_for`, read by `assemble`"
)]
pub(in crate::backends::rust) enum Landing {
    /// Into the one block of the same header: inserted at byte `before`, the start of the line
    /// holding its closing `}`.
    Join { before: usize },
    /// Into a new block, placed by `item_move::placement::insertion`.
    New,
}

/// The landing of members whose block is written `header` in the module spanning `scope` of
/// `destination_text`.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): called by `assemble` once it is implemented"
)]
pub(in crate::backends::rust) fn block_for(
    destination_text: &str,
    scope: &Range<usize>,
    header: &str,
) -> Landing {
    // TODO(reshape-move-impl-members): implement R3's join rule.
    let _ = (destination_text, scope, header);
    todo!("TODO(reshape-move-impl-members): implement R3's join rule")
}
