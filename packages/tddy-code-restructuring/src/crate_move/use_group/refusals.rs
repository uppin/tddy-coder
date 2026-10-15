//! The refusals of the group rule: a group that would repeat an attribute on every line it splits
//! into, and a `use` the rule cannot read.

// TODO(reshape-8-hand-fix): `crate_move`'s own `malformed`, not `repoint_facade`'s — a move may not
// name `backends`.
use crate::crate_move::malformed;
use crate::RestructureError;

/// A `use` group an attribute or doc comment gates would have to be split into several statements,
/// repeating the attribute.
pub(crate) fn attribute_above_a_split(file: &str, line: u32) -> RestructureError {
    malformed(format!(
        "{file}:{line}: this `use` group carries an attribute or doc comment and would have to be \
         split into one statement per member, which would repeat the attribute — write one `use` \
         per path"
    ))
}

/// A `use` item this operation cannot read as one statement, left as it is rather than guessed at.
pub(crate) fn unreadable_use(statement: &str) -> RestructureError {
    malformed(format!(
        "`{statement}` is a `use` this operation cannot read, so it is left as it is rather than \
         guessed at"
    ))
}
