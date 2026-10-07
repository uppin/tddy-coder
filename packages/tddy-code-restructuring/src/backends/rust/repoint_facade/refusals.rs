//! The refusals of `repoint_facade_imports` that need the code: an undeclared defining crate, a
//! rename in a body, a path spelled across whitespace or a comment, a duplicate binding, an
//! attribute above a group that must split, and an anchor that names no module.
//!
//! Every one names what made it refuse — the path, the file and the line where it is written — so a
//! reader can find the line rather than only learn that the operation declined.

use crate::RestructureError;

fn malformed(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}

/// A range anchor that covers no `mod` declaration names no module to re-point.
pub(super) fn no_module_in(file: &str, line: u32) -> RestructureError {
    malformed(format!(
        "{file}:{line}: this range covers no `mod` declaration, so it names no module to re-point \
         — anchor the operation on a module's `mod` declaration, or on the file itself"
    ))
}

/// The defining crate is one the package's manifest does not declare, and adding a dependency is
/// the cross-crate moves' pass rather than this one's.
pub(super) fn undeclared_crate(file: &str, line: u32, path: &str, krate: &str) -> RestructureError {
    malformed(format!(
        "{file}:{line}: `{path}` is defined in `{krate}`, which this package's `Cargo.toml` does \
         not declare — add the dependency, or leave the path as it is"
    ))
}

/// A path spelled across whitespace or with a comment inside it cannot be addressed by a byte
/// replacement, so it is refused rather than guessed at.
pub(super) fn spelled_across(file: &str, line: u32, path: &str) -> RestructureError {
    malformed(format!(
        "{file}:{line}: `{path}` is spelled across whitespace or a comment, which the path rewrite \
         cannot address — write it on one line"
    ))
}

/// A facade that renames its item cannot be followed in a body: the token that changes would be
/// neither a re-point a reader expects nor one `verify` excuses.
pub(super) fn renamed_in_a_body(file: &str, path: &str) -> RestructureError {
    malformed(format!(
        "{file}: `{path}` is renamed by the facade it goes through, and a path in a body cannot be \
         renamed — name it where it is defined, or import it under a `use`"
    ))
}

/// Re-pointing a `use` onto a path whose name is already bound in the same scope would be `E0252`.
pub(super) fn binds_a_name_twice(
    file: &str,
    line: u32,
    path: &str,
    other: &str,
) -> RestructureError {
    malformed(format!(
        "{file}:{line}: re-pointing `{path}` would bind a name the same scope already binds \
         through `{other}`, which is `E0252` — remove one of the two"
    ))
}

/// A `use` group an attribute or doc comment gates would have to be split into several statements,
/// repeating the attribute.
pub(super) fn attribute_above_a_split(file: &str, line: u32) -> RestructureError {
    malformed(format!(
        "{file}:{line}: this `use` group carries an attribute or doc comment and would have to be \
         split into one statement per member, which would repeat the attribute — write one `use` \
         per path"
    ))
}

/// A nested group member whose leaves reach two crates cannot be lifted whole under one prefix.
pub(super) fn nested_member_reaches_two_crates(member: &str) -> RestructureError {
    malformed(format!(
        "the member `{member}` of a grouped `use` reaches two crates, so the group cannot carry \
         one prefix — write one `use` per path"
    ))
}

/// A `use` item this operation cannot read as one statement, left as it is rather than guessed at.
pub(super) fn unreadable_use(statement: &str) -> RestructureError {
    malformed(format!(
        "`{statement}` is a `use` this operation cannot read, so it is left as it is rather than \
         guessed at"
    ))
}
