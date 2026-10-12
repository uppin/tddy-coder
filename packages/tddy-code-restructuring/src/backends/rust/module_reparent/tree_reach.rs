//! The items of the modules a re-parented tree leaves that it still names.
//!
//! A child reaches its parent's private items; a module under another parent does not. So the tree
//! that moves can lose sight of an item of its old parent, or of an ancestor, that it calls through
//! `super::`. Only the ancestors the old and new places do not share can hold such an item: one that
//! is also an ancestor of the new place stays visible ([`modules_to_survey`]). The server confirms
//! each candidate the moved files mention, and the item is widened only as far as the module's new
//! path needs ([`widenings`]), the way `move_item` widens an item it leaves behind.

use std::collections::BTreeMap;

use super::super::item_move::destination::{find_module, Lookup};
use super::super::item_move::outline::{self, visibility_edit, Item};
use super::super::item_move::scope::Scope;
use super::super::item_move::text::{identifiers_in, Edit};
use super::super::{seam_refusal, uri_of, RustBackend};
use super::reading::Reparent;
use super::survey::Survey;
use crate::edit::VisibilityChange;
use crate::item_anchor::module_path_of;
use crate::registry::Workspace;
use crate::Result;
use serde_json::Value;

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
    let shared = old_parent
        .iter()
        .zip(new_parent)
        .take_while(|(old, new)| old == new)
        .count();
    (shared + 1..=old_parent.len())
        .rev()
        .map(|depth| old_parent[..depth].to_vec())
        .collect()
}

/// The edits that widen each reached item, in its own file, until `new_module` can see it, and the
/// report of each.
pub(super) fn widenings(
    texts: &BTreeMap<String, String>,
    reached: &[ReachedItem],
    new_module: &[String],
) -> Result<Widened> {
    let mut edits = Vec::new();
    let mut report = Vec::new();
    for reached in reached {
        let item = &reached.item;
        let written = read_scope(item, &reached.module)?;
        let widened = written.clone().widened_to(new_module);
        if widened == written {
            continue;
        }
        let to = widened.spelled_in(&reached.module);
        let edit = visibility_edit(&texts[&reached.file], &item.position, &item.name, &to)?;
        edits.push((reached.file.clone(), edit));
        report.push(VisibilityChange {
            item: item.name.clone(),
            from: as_written(&item.visibility),
            to: as_written(&to),
        });
    }
    Ok((edits, report))
}

fn read_scope(item: &Item, module: &[String]) -> Result<Scope> {
    Scope::parse(&item.visibility, module).ok_or_else(|| {
        seam_refusal(format!(
            "the visibility `{}` of `{}` is not one this move can read",
            item.visibility, item.name
        ))
    })
}

fn as_written(visibility: &str) -> String {
    if visibility.is_empty() {
        "private".to_string()
    } else {
        visibility.to_string()
    }
}

/// Whether any of the server's `references` lies in one of the files at `moved_uris`.
fn from_a_moved_file(references: &Value, moved_uris: &[String]) -> bool {
    references
        .as_array()
        .into_iter()
        .flatten()
        .any(|reference| {
            reference
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| moved_uris.iter().any(|moved| moved == uri))
        })
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
        let moved: String = survey
            .files
            .iter()
            .map(|file| workspace.read(&file.from))
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        let mentioned = identifiers_in(&moved);
        let moved_uris: Vec<String> = survey
            .files
            .iter()
            .map(|file| uri_of(&workspace.root.join(&file.from)))
            .collect();
        let mut new_module = survey.new_parent.path.clone();
        new_module.push(request.name.clone());

        let mut reached = Vec::new();
        for module in modules_to_survey(&request.parent, &survey.new_parent.path) {
            let Lookup::Found(found) = find_module(workspace, &request.named.package, &module)?
            else {
                continue;
            };
            // Only a module that is a file of its own has root items that are the file's.
            if module_path_of(workspace.root, &found.file)?.split_off(1) != module {
                continue;
            }
            let text = workspace.read(&found.file)?;
            let uri = uri_of(&workspace.root.join(&found.file));
            self.did_open(&uri, &text)?;
            let symbols = self.settled_outline(&uri)?;
            for item in outline::root_items(&symbols, &text, None) {
                let names_the_module = module == request.parent && item.name == request.name;
                if names_the_module
                    || !mentioned.contains(&item.name)
                    || read_scope(&item, &module)?.widened_to(&new_module)
                        == read_scope(&item, &module)?
                {
                    continue;
                }
                let references = self.references_at(&uri, &item.position)?;
                if from_a_moved_file(&references, &moved_uris) {
                    reached.push(ReachedItem {
                        file: found.file.clone(),
                        module: module.clone(),
                        item,
                    });
                }
            }
        }
        Ok(reached)
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
