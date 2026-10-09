//! Rewriting one `use` statement whose leaves a move re-points: Rule P (re-prefix in place when every
//! leaf agrees) and Rule S (kept members first, then one `use` per lifted member; a nested member
//! whose leaves disagree is flattened).
//!
//! One rule for every caller: a cross-crate move's own header, the callers it re-points with
//! `reexport: none`, and `repoint_facade_imports`. It lives here, below `backends`, because
//! `crate_move` must not depend on `backends::rust` — the move reads it, and `repoint_facade` reaches
//! down for it.
//!
//! `split_or_reprefix` and the text helpers it reads (`members_of`, `split_use`, `use_statements`)
//! arrive in this module by an engine `move_item` from `backends::rust::repoint_facade::group` and
//! `backends::rust::item_move` (`#reshape` 8/19, milestone M1); this file holds what is new.

use std::ops::Range;

use crate::edit::TextEdit;
use crate::Result;

/// One leaf of a `use` statement: the path as written, and the path it is written as afterwards.
///
/// `rewritten == written` marks a leaf that keeps its path — a **kept** member under Rule S.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "TODO(reshape-move-grouped-use): read by `split_or_reprefix` once it moves here"
)]
pub(crate) struct Leaf {
    /// The whole path, from the statement's own head: `crate::connection_service::seed_codebase`.
    pub(crate) written: String,
    /// What it becomes: `crate::seed_codebase`.
    pub(crate) rewritten: String,
}

/// The one edit that replaces the `use` statement at `span` of `file`'s `text` with `leaves` applied
/// — the prefix alone when Rule P holds, the whole statement when Rule S splits it, every lifted line
/// carrying the statement's own indentation.
///
/// # Errors
///
/// Refuses a statement that must split while an attribute or doc comment sits on the line above it
/// (`refusals::attribute_above_a_split`), and one the rule cannot read.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-grouped-use): called by `header::rewrite_of` and \
              `moving::caller_changes` in the green phase"
)]
pub(crate) fn statement_edit(
    file: &str,
    text: &str,
    span: Range<usize>,
    leaves: &[Leaf],
) -> Result<TextEdit> {
    // TODO(reshape-move-grouped-use): implement — `group_rewrite`'s attribute check and
    // re-indentation over `split_or_reprefix`.
    let _ = (file, text, span, leaves);
    todo!("use_group::statement_edit")
}

/// The span of the `use` statement of `text` that holds byte offset `at` — from its visibility (or
/// its keyword) to the `;` that ends it — or `None` when `at` is in no `use` statement.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-grouped-use): called by `header::rewrite_of` and \
              `moving::caller_changes` in the green phase"
)]
pub(crate) fn statement_containing(text: &str, at: usize) -> Option<Range<usize>> {
    // TODO(reshape-move-grouped-use): implement — `use_statements` over the masked text.
    let _ = (text, at);
    todo!("use_group::statement_containing")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_leaf(written: &str, rewritten: &str) -> Leaf {
        Leaf {
            written: written.to_string(),
            rewritten: rewritten.to_string(),
        }
    }

    #[test]
    fn finds_the_statement_holding_an_offset_from_its_visibility_to_its_semicolon() {
        // Given a `pub use` group after a plain `use`
        let text = "use std::fmt;\npub use crate::{a::X, b::Y};\n";
        let inside = text.find("b::Y").expect("the leaf is written");

        // When the statement holding a leaf of the group is asked for
        let span = statement_containing(text, inside);

        // Then it is the whole `pub use` statement
        assert_eq!(
            span.map(|span| &text[span]),
            Some("pub use crate::{a::X, b::Y};")
        );
    }

    #[test]
    fn an_offset_in_no_use_statement_has_no_statement() {
        // Given a body path
        let text = "fn f() -> u32 {\n    crate::a::count()\n}\n";
        let inside = text.find("count").expect("the path is written");

        // When
        let span = statement_containing(text, inside);

        // Then
        assert_eq!(span, None);
    }

    #[test]
    fn a_statement_edit_lifts_a_disagreeing_member_with_the_statements_indentation() {
        // Given a group inside an inline module whose second leaf lands in another crate
        let text = "mod inner {\n    use crate::{a::X, clock::Clock};\n}\n";
        let span = statement_containing(text, text.find("clock").expect("the leaf"))
            .expect("the statement");
        let leaves = [
            a_leaf("crate::a::X", "crate::a::X"),
            a_leaf("crate::clock::Clock", "shared::clock::Clock"),
        ];

        // When the statement is rewritten
        let edit = statement_edit("src/inner.rs", text, span, &leaves).expect("the edit");

        // Then the kept member stays first and the lifted one follows at the same indentation
        assert_eq!(
            crate::apply::edited(text.to_string(), &[edit]).expect("the edit applies"),
            "mod inner {\n    use crate::{a::X};\n    use shared::clock::Clock;\n}\n"
        );
    }
}
