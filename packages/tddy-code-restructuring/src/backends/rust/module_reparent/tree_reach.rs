//! The items of the modules a re-parented tree leaves that it still names.
//!
//! A child reaches its parent's private items; a module under another parent does not. So the tree
//! that moves can lose sight of an item of its old parent, or of an ancestor, that it calls through
//! `super::`. Only the ancestors the old and new places do not share can hold such an item: one that
//! is also an ancestor of the new place stays visible ([`modules_to_survey`]). The server confirms
//! each candidate the moved files mention, and the item is widened only as far as the module's new
//! path needs ([`widenings`]), the way `move_item` widens an item it leaves behind.

// TODO(reshape-widen-same-crate): remove once `reparent_module` surveys the tree's reach (M6).
#![allow(dead_code)]

use std::collections::BTreeMap;

use super::super::item_move::outline::Item;
use super::super::item_move::text::Edit;
use super::super::RustBackend;
use super::reading::Reparent;
use super::survey::Survey;
use crate::edit::VisibilityChange;
use crate::registry::Workspace;
use crate::Result;

/// Edits to files (keyed by path) and the report of every widening they make.
pub(super) type Widened = (Vec<(String, Edit)>, Vec<VisibilityChange>);

/// An item of a module the tree leaves, which the moved files name.
#[derive(Debug, Clone)]
pub(super) struct ReachedItem {
    /// The file that declares it.
    pub(super) file: String,
    /// The module it is declared in, below the crate root.
    pub(super) module: Vec<String>,
    pub(super) item: Item,
}

/// The modules whose items the tree may stop seeing when it moves from under `old_parent` to under
/// `new_parent`: the old parent and every ancestor strictly below the two places' common ancestor,
/// nearest first.
pub(super) fn modules_to_survey(old_parent: &[String], new_parent: &[String]) -> Vec<Vec<String>> {
    let _ = (old_parent, new_parent);
    // TODO(reshape-widen-same-crate): implement
    todo!("the old parent and its ancestors below the common ancestor")
}

/// The edits that widen each reached item, in its own file, until `new_module` can see it, and the
/// report of each.
pub(super) fn widenings(
    texts: &BTreeMap<String, String>,
    reached: &[ReachedItem],
    new_module: &[String],
) -> Result<Widened> {
    let _ = (texts, reached, new_module);
    // TODO(reshape-widen-same-crate): implement
    todo!("widen each reached item to cover the module's new path")
}

impl RustBackend {
    /// The items of [`modules_to_survey`] that the moved files name, confirmed by the server: a
    /// reference to each lands in a moved file.
    pub(super) fn reached_by_the_tree(
        &mut self,
        workspace: &Workspace<'_>,
        request: &Reparent,
        survey: &Survey,
    ) -> Result<Vec<ReachedItem>> {
        let _ = (workspace, request, survey);
        // TODO(reshape-widen-same-crate): implement
        todo!("survey the old parent and ancestors for the items the tree names")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &str) -> Vec<String> {
        path.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn modules(paths: &[&str]) -> Vec<Vec<String>> {
        paths.iter().map(|path| module(path)).collect()
    }

    #[test]
    fn surveys_the_old_parent_and_the_ancestors_below_the_common_ancestor_and_nothing_above_it() {
        // Given a move between siblings, a move between cousins, and a move into the old parent's
        // own subtree
        let between_siblings = modules_to_survey(&module("host"), &module("split"));
        let between_cousins = modules_to_survey(&module("a::b::host"), &module("a::split"));
        let into_its_own_parent = modules_to_survey(&module("a::host"), &module("a::host::x"));

        // Then only the ancestors the new place does not share are surveyed, nearest first
        assert_eq!(between_siblings, modules(&["host"]));
        assert_eq!(between_cousins, modules(&["a::b::host", "a::b"]));
        assert_eq!(into_its_own_parent, Vec::<Vec<String>>::new());
    }
}
