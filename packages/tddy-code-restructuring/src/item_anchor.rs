//! Item anchors: from a crate-rooted item path to the exact coordinates the ledger translates.
//!
//! A plan's `item` and `items` anchors name *what* they act on, not *where* it sat when the plan was
//! written. They are resolved once, at run open, against the tree the run starts on, and lowered
//! into the `range` and `symbol` anchors every operation already understands. From there the
//! [`crate::PositionLedger`] carries them through the run exactly as it carries a hand-written range.
//!
//! The language-specific half — reading an outline, walking it by segment — lives behind
//! [`ItemResolver`]. What is here is what every language shares: which module a file is, how a
//! relative range becomes an absolute one, and when a resolved item is refused.

use std::path::Path;

use crate::edit::{Position, Range};
use crate::plan::{Anchor, Fingerprint, ItemPath, Plan};
use crate::Result;

/// An item the language server located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    /// The item's full extent, attributes and doc comments included, in one-based coordinates.
    pub range: Range,
    /// Where the item's name is — the position symbol operations act on.
    pub name: Position,
    /// The fingerprint of the item's text as the tree holds it now.
    pub fingerprint: Fingerprint,
}

/// A backend that can find an item by its path in one file.
pub trait ItemResolver {
    /// Locate `item` in `file` (relative to the workspace root).
    ///
    /// Refuses — never guesses — when the file's module path does not match the item's prefix, when
    /// a segment is absent, or when a segment matches more than one outline node. Nothing searches a
    /// file other than `file`.
    fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem>;
}

/// The module path of a source file inside its package, as `[crate, module, …]`.
///
/// The crate is the package's `name` with `-` read as `_`; `src/lib.rs` and `src/main.rs` are the
/// crate root, and `a/mod.rs` and `a.rs` are both module `a`. A file outside `src/` belongs to no
/// module path this can name and is refused.
pub fn module_path_of(root: &Path, file: &str) -> Result<Vec<String>> {
    // TODO(item-anchors): implement
    let _ = (root, file);
    todo!("item-anchors: the module path of a file")
}

/// The absolute range an item anchor names, given where its item was found.
///
/// `start`/`end` absent means the item itself, at its name. A range reaching outside the item is
/// refused as malformed.
pub fn absolute_range(
    resolved: &ResolvedItem,
    start: Option<Position>,
    end: Option<Position>,
) -> Result<Range> {
    // TODO(item-anchors): implement
    let _ = (resolved, start, end);
    todo!("item-anchors: a relative range made absolute")
}

/// Every item anchor in `plan` resolved against the tree under `root` and lowered into the
/// snapshot coordinates the ledger translates.
///
/// Every `v1` anchor passes through untouched, so a plan with no item anchors comes back equal.
/// An item whose fingerprint no longer matches is refused with
/// [`crate::RestructureError::ItemChanged`], naming it.
pub fn resolve_item_anchors(
    plan: &Plan,
    root: &Path,
    resolver: &mut dyn ItemResolver,
) -> Result<Plan> {
    // TODO(item-anchors): implement
    let _ = (plan, root, resolver);
    todo!("item-anchors: resolve and lower every item anchor at run open")
}

/// The item anchor for the innermost item enclosing `range` in `file` — what `anchors --at` emits.
pub fn item_anchor_at(
    root: &Path,
    file: &str,
    range: Range,
    resolver: &mut dyn ItemAtResolver,
) -> Result<Anchor> {
    // TODO(item-anchors): implement
    let _ = (root, file, range, resolver);
    todo!("item-anchors: the item anchor enclosing a position")
}

/// A backend that can name the innermost item enclosing a position.
pub trait ItemAtResolver {
    /// The path of the innermost item in `file` whose full range contains `range`, and where it is.
    fn item_enclosing(&mut self, file: &str, range: Range) -> Result<(ItemPath, ResolvedItem)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_resolved_item(first_line: u32, last_line: u32) -> ResolvedItem {
        ResolvedItem {
            range: Range {
                start: Position {
                    line: first_line,
                    col: 5,
                },
                end: Position {
                    line: last_line,
                    col: 6,
                },
            },
            name: Position {
                line: first_line,
                col: 12,
            },
            fingerprint: Fingerprint("sha256:ab".to_string()),
        }
    }

    fn a_package(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let absolute = root.path().join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(absolute, text).unwrap();
        }
        root
    }

    const MANIFEST: &str = "[package]\nname = \"tddy-core\"\nversion = \"0.1.0\"\n";

    #[test]
    fn a_relative_range_is_counted_from_the_items_first_line() {
        // Given an item on lines 40–50
        let item = a_resolved_item(40, 50);

        // When its lines 2–3 are made absolute
        let range = absolute_range(
            &item,
            Some(Position { line: 2, col: 9 }),
            Some(Position { line: 3, col: 33 }),
        );

        // Then they are lines 41–42, columns unchanged
        assert_eq!(
            range.ok(),
            Some(Range {
                start: Position { line: 41, col: 9 },
                end: Position { line: 42, col: 33 },
            })
        );
    }

    #[test]
    fn no_relative_range_names_the_item_at_its_name() {
        let item = a_resolved_item(40, 50);

        assert_eq!(
            absolute_range(&item, None, None).ok(),
            Some(Range {
                start: Position { line: 40, col: 12 },
                end: Position { line: 40, col: 12 },
            })
        );
    }

    #[test]
    fn a_relative_range_past_the_items_last_line_is_refused() {
        let item = a_resolved_item(40, 44);

        let refused = absolute_range(
            &item,
            Some(Position { line: 2, col: 9 }),
            Some(Position { line: 9, col: 1 }),
        );

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn a_file_under_src_is_the_module_its_path_names() {
        // Given a package named `tddy-core`
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/workflow/stack.rs", ""),
        ]);

        // When the module path of a nested file is asked for
        let path = module_path_of(root.path(), "packages/core/src/workflow/stack.rs");

        // Then the crate reads `-` as `_` and the directories are modules
        assert_eq!(
            path.ok(),
            Some(vec![
                "tddy_core".to_string(),
                "workflow".to_string(),
                "stack".to_string()
            ])
        );
    }

    #[test]
    fn a_mod_rs_is_the_module_its_directory_names() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/workflow/mod.rs", ""),
        ]);

        assert_eq!(
            module_path_of(root.path(), "packages/core/src/workflow/mod.rs").ok(),
            Some(vec!["tddy_core".to_string(), "workflow".to_string()])
        );
    }

    #[test]
    fn lib_rs_is_the_crate_root() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/lib.rs", ""),
        ]);

        assert_eq!(
            module_path_of(root.path(), "packages/core/src/lib.rs").ok(),
            Some(vec!["tddy_core".to_string()])
        );
    }

    #[test]
    fn a_file_outside_src_is_refused() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/tests/golden.rs", ""),
        ]);

        assert!(matches!(
            module_path_of(root.path(), "packages/core/tests/golden.rs"),
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }
}
