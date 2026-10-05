//! Rewriting one `use` statement: Rule P (re-prefix in place when every member agrees) and Rule S
//! (kept members first, then one `use` per lifted member).
//!
//! TODO(repoint-facade): implement; unused until `resolve` calls it.

#![allow(dead_code)]

use super::refusals::unfinished;
use super::rewrite::Rewrite;
use crate::Result;

/// `statement`, one whole `use` item, with `leaves` (its rewritten paths) applied.
///
/// TODO(repoint-facade): implement.
pub(super) fn split_or_reprefix(_statement: &str, _leaves: &[Rewrite]) -> Result<String> {
    Err(unfinished())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_leaf(written: &str, defined_at: &str) -> Rewrite {
        Rewrite {
            written: written.to_string(),
            defined_at: defined_at.to_string(),
            line: 1,
            split_from_group: false,
        }
    }

    /// Rule P: when every member agrees on what the common prefix becomes, the prefix is replaced in
    /// place and the group keeps its shape.
    #[test]
    fn a_group_whose_members_agree_keeps_its_shape_under_the_new_prefix() {
        // Given a group whose members both go through `crate::config`
        let leaves = [
            a_leaf("crate::config::A", "kernel::config::A"),
            a_leaf("crate::config::B", "kernel::config::B"),
        ];

        // When it is re-pointed
        let statement = split_or_reprefix("use crate::config::{A, B};", &leaves);

        // Then the prefix is replaced in place
        assert_eq!(
            statement.expect("the group is re-pointed"),
            "use kernel::config::{A, B};"
        );
    }

    /// Rule S: members that need different qualifiers leave the group, kept members first.
    #[test]
    fn a_group_whose_members_disagree_keeps_its_kept_members_first_and_lifts_the_rest() {
        // Given a group of one member through a facade and one of the crate's own
        let leaves = [a_leaf("crate::config::Limits", "kernel::config::Limits")];

        // When it is split
        let statement = split_or_reprefix("use crate::{config::Limits, b::Thing};", &leaves);

        // Then the kept member stays and the lifted one follows on its own line
        assert_eq!(
            statement.expect("the group is split"),
            "use crate::{b::Thing};\nuse kernel::config::Limits;"
        );
    }

    /// A group with no kept member has nothing left to stand in.
    #[test]
    fn a_group_with_no_kept_member_disappears_into_its_lifted_statements() {
        // Given a group whose members are re-exported from two crates, behind a visibility
        let leaves = [
            a_leaf("crate::config::Limits", "kernel::config::Limits"),
            a_leaf("crate::roster::Roster", "agents::roster::Roster"),
        ];

        // When it is split
        let statement =
            split_or_reprefix("pub use crate::{config::Limits, roster::Roster};", &leaves);

        // Then one public statement per member remains
        assert_eq!(
            statement.expect("the group is split"),
            "pub use kernel::config::Limits;\npub use agents::roster::Roster;"
        );
    }
}
