//! The `use` items the moved code needs where it lands.
//!
//! The items leave the module whose `use` items gave their names meaning. Two things restore it, and
//! both are read from the source module's own text rather than guessed:
//!
//! - the module's `use` items, copied across with their attributes, the qualifier at their head
//!   (`self`, `super`, a module the source declares) written from the crate root so it means the same
//!   thing in the new module, and any that the destination already binds left out;
//! - one `use` for each item the source module keeps that the moved code names, which the server's
//!   reference set says it does.
//!
//! Copying the whole header over-imports: a trait import has no name in the code that needs it, so
//! no reading of the moved text could tell which to leave out. The surplus is what the unused-import
//! tidy at the end of a complete run removes, on the compiler's own evidence.

use std::collections::BTreeSet;
use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::text::{depth_at, split_use, use_statements};
use crate::crate_move::source_scan::{items_of_module, UseLeaf};

/// The source module, as the code being moved sees it.
pub(super) struct Source<'a> {
    pub(super) text: &'a str,
    pub(super) scope: Range<usize>,
    pub(super) module: &'a [String],
    /// How the source module is named from the crate root: `crate::pairing`.
    pub(super) qualifier: &'a str,
}

/// The `use` items to write into the destination.
pub(super) fn needed(
    source: &Source<'_>,
    taken: &BTreeSet<String>,
    left_behind_named: &[String],
) -> Vec<String> {
    let mut lines = header_imports(source, taken);
    lines.extend(
        left_behind_named
            .iter()
            .filter(|name| !taken.contains(*name))
            .map(|name| format!("use {}::{name};", source.qualifier)),
    );
    lines
}

fn header_imports(source: &Source<'_>, taken: &BTreeSet<String>) -> Vec<String> {
    let masked = masked_to_code(source.text);
    let base = depth_at(&masked, source.scope.start);
    let siblings = items_of_module(&source.text[source.scope.clone()]);
    let local: BTreeSet<&str> = siblings
        .children
        .iter()
        .map(|child| child.name.as_str())
        .chain(siblings.defined.iter().map(String::as_str))
        .collect();

    let mut lines = Vec::new();
    for span in use_statements(&masked) {
        if span.start < source.scope.start
            || span.end > source.scope.end
            || depth_at(&masked, span.start) != base
        {
            continue;
        }
        let Some((_, tree)) = split_use(&source.text[span.clone()]) else {
            continue;
        };
        let rebased = rebased_tree(tree, source, &local);
        let statement = format!("use {rebased};");
        let leaves = items_of_module(&statement).uses;
        let bound = |leaf: &UseLeaf| leaf.bound_name().is_some_and(|name| taken.contains(name));
        if leaves.iter().any(bound) {
            // A group the destination binds part of: what it does not bind, one `use` each, because
            // writing the whole group again would bind the rest twice (`E0252`).
            let attributes = attributes_above(source.text, span.start);
            lines.extend(
                leaves
                    .iter()
                    .filter(|leaf| !bound(leaf))
                    .map(|leaf| format!("{attributes}use {};", spelled(leaf))),
            );
            continue;
        }
        lines.push(format!(
            "{}{statement}",
            attributes_above(source.text, span.start)
        ));
    }
    lines
}

/// A leaf of a `use` item, written as an item of its own.
fn spelled(leaf: &UseLeaf) -> String {
    let path = leaf.segments.join("::");
    match (&leaf.alias, leaf.glob) {
        (_, true) => format!("{path}::*"),
        (Some(alias), false) => format!("{path} as {alias}"),
        (None, false) => path,
    }
}

/// The attribute lines written directly above the item at `at`, each ending in a newline.
fn attributes_above(text: &str, at: usize) -> String {
    let own_line = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let mut attributes: Vec<&str> = text[..own_line]
        .lines()
        .rev()
        .map(str::trim)
        .take_while(|line| line.starts_with("#["))
        .collect();
    attributes.reverse();
    attributes.iter().map(|line| format!("{line}\n")).collect()
}

/// `tree` with the module its first segment is relative to written out from the crate root.
fn rebased_tree(tree: &str, source: &Source<'_>, local: &BTreeSet<&str>) -> String {
    let mut module = source.module.to_vec();
    let mut rest = tree;
    let mut relative = false;
    loop {
        if let Some(after) = rest.strip_prefix("self::") {
            rest = after;
            relative = true;
        } else if let Some(after) = rest.strip_prefix("super::") {
            if module.pop().is_none() {
                return tree.to_string();
            }
            rest = after;
            relative = true;
        } else {
            break;
        }
    }
    if !relative {
        let first: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if first == "crate" || !local.contains(first.as_str()) {
            return tree.to_string();
        }
        module = source.module.to_vec();
    }
    if module.is_empty() {
        format!("crate::{rest}")
    } else {
        format!("crate::{}::{rest}", module.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &[&str]) -> Vec<String> {
        path.iter().map(|segment| segment.to_string()).collect()
    }

    fn the_header_of(text: &str, module: &[String]) -> Vec<String> {
        let source = Source {
            text,
            scope: 0..text.len(),
            module,
            qualifier: "crate::pairing",
        };
        needed(&source, &BTreeSet::new(), &[])
    }

    #[test]
    fn writes_a_relative_import_from_the_crate_root() {
        let found = the_header_of(
            "use super::shared::Clock;\nuse self::helper::f;\n\nmod helper;\n",
            &module(&["a", "pairing"]),
        );

        assert_eq!(
            found,
            [
                "use crate::a::shared::Clock;",
                "use crate::a::pairing::helper::f;"
            ]
        );
    }

    #[test]
    fn leaves_an_import_from_another_crate_as_written() {
        let found = the_header_of("use std::fmt::Write;\n", &module(&["pairing"]));

        assert_eq!(found, ["use std::fmt::Write;"]);
    }

    #[test]
    fn carries_the_attribute_of_an_import() {
        let found = the_header_of(
            "#[cfg(test)]\nuse std::fmt::Write;\n",
            &module(&["pairing"]),
        );

        assert_eq!(found, ["#[cfg(test)]\nuse std::fmt::Write;"]);
    }

    #[test]
    fn leaves_out_an_import_of_a_name_the_destination_already_binds() {
        let source = Source {
            text: "use std::fmt::Write;\n",
            scope: 0..21,
            module: &module(&["pairing"]),
            qualifier: "crate::pairing",
        };
        let taken = BTreeSet::from(["Write".to_string()]);

        assert!(needed(&source, &taken, &[]).is_empty());
    }

    #[test]
    fn writes_only_what_the_destination_does_not_bind_of_a_group() {
        let source = Source {
            text: "use tddy_rpc::{Response, Status};\n",
            scope: 0..34,
            module: &module(&["pairing"]),
            qualifier: "crate::pairing",
        };
        let taken = BTreeSet::from(["Status".to_string()]);

        assert_eq!(needed(&source, &taken, &[]), ["use tddy_rpc::Response;"]);
    }

    #[test]
    fn imports_what_the_source_module_keeps_and_the_moved_code_names() {
        let source = Source {
            text: "",
            scope: 0..0,
            module: &module(&["pairing"]),
            qualifier: "crate::pairing",
        };

        assert_eq!(
            needed(&source, &BTreeSet::new(), &["limit".to_string()]),
            ["use crate::pairing::limit;"]
        );
    }
}
