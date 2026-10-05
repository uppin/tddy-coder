//! `verify` already accounts for what `repoint_facade_imports` writes, and reports what it would
//! have to refuse.
//!
//! The operation changes the head of a path, never what the path names: `crate::config::Limits`
//! becomes `kernel::config::Limits`. `verify`'s re-point pass pairs a lost statement with a gained
//! one when the two are equal once their lowercase module qualifiers are deleted, and a `use` item
//! is scaffolding it does not read at all, so neither a re-pointed body path nor a split group
//! needs a declaration. What the pass does not excuse is a changed name, which is why the operation
//! refuses to rename in a body.
//!
//! These tests pin what already holds: they are green before the operation exists, and they keep
//! the operation honest about what a later change to `verify` must not break.
//!
//! Library level: `verify::compare` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare, Comparison};

fn a_crate_holding(source: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("src/a.rs".to_string(), source.to_string())])
}

fn comparing(before: &str, after: &str) -> Comparison {
    compare(&a_crate_holding(before), &a_crate_holding(after))
}

#[test]
fn a_body_path_and_a_called_function_re_pointed_through_a_facade_are_accounted_for_and_counted_as_repointed(
) {
    // Given a body whose path to a type and whose called function were re-pointed to their defining crate
    let before = concat!(
        "use crate::config::Settings;\n",
        "pub fn limit(s: &Settings) -> u32 {\n",
        "    let l = crate::config::standard_limits();\n",
        "    let d = crate::config::Settings::default_verbose();\n",
        "    l.max + d as u32 + s.verbose as u32\n",
        "}\n",
    );
    let after = concat!(
        "use kernel::config::Settings;\n",
        "pub fn limit(s: &Settings) -> u32 {\n",
        "    let l = kernel::config::standard_limits();\n",
        "    let d = kernel::config::Settings::default_verbose();\n",
        "    l.max + d as u32 + s.verbose as u32\n",
        "}\n",
    );

    // When the two are compared
    let comparison = comparing(before, after);

    // Then nothing is missing or added, and the two body statements are counted as re-pointed
    assert!(
        comparison.holds(),
        "the re-point was reported: {comparison:?}"
    );
    assert_eq!(comparison.excused.repointed, 2, "{comparison:?}");
}

#[test]
fn a_split_group_changes_nothing_verify_reads_and_a_renamed_item_is_still_reported() {
    // Given a group that was split, and in a second pair a body that now names the item by another name
    let (grouped, split) = (
        "use crate::{config::Limits, b::Thing};\n",
        "use crate::{b::Thing};\nuse kernel::config::Limits;\n",
    );
    let (named_by_the_facade, named_by_the_definition) = (
        "pub fn s() -> u32 {\n    crate::AppSettings::standard().verbose as u32\n}\n",
        "pub fn s() -> u32 {\n    kernel::config::Settings::standard().verbose as u32\n}\n",
    );

    // When each pair is compared
    let split_group = comparing(grouped, split);
    let renamed = comparing(named_by_the_facade, named_by_the_definition);

    // Then the split group is accounted for, and the renamed item is a statement lost and one gained
    assert!(
        split_group.holds(),
        "the split was reported: {split_group:?}"
    );
    assert_eq!(
        (renamed.missing, renamed.added),
        (
            vec!["crate::AppSettings::standard().verbose as u32".to_string()],
            vec!["kernel::config::Settings::standard().verbose as u32".to_string()]
        )
    );
}
