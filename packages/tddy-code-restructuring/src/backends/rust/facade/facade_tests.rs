//! What a facade an extraction leaves re-exports: the public path stays whole, and names only the
//! parent's tests reach are re-exported for the test build alone.

use super::*;

/// How something outside the new module reaches a moved item.
#[derive(Clone, Copy)]
enum Reached {
    Nowhere,
    FromProduction,
    OnlyFromTests,
}

fn a_moved(name: &str, visibility: &str, reached: Reached) -> seam_survey::MovedItem {
    seam_survey::MovedItem {
        name: name.to_string(),
        within: Vec::new(),
        visibility: visibility.to_string(),
        stranded_in: Vec::new(),
        reached_from_outside: !matches!(reached, Reached::Nowhere),
        reached_from_production: matches!(reached, Reached::FromProduction),
        referenced_in_impl_at: Vec::new(),
    }
}

fn the_named_facade_of(items: &[seam_survey::MovedItem]) -> Vec<String> {
    facade_lines("rendering", items, Reexport::Named).expect("a named facade is written")
}

#[test]
fn a_named_facade_re_exports_an_unreferenced_pub_item_on_the_pub_line() {
    // Given a seam holding a `pub fn` nothing calls, beside one something does call
    let items = [
        a_moved("render", "pub", Reached::FromProduction),
        a_moved("statements", "pub", Reached::Nowhere),
    ];

    // When its named facade is written
    let facade = the_named_facade_of(&items);

    // Then the unreferenced item keeps its public path
    assert_eq!(facade, ["pub use rendering::{render, statements};"]);
}

#[test]
fn a_named_facade_still_leaves_out_an_unreferenced_pub_crate_item() {
    // Given a seam holding a `pub(crate)` helper nothing outside the seam calls
    let items = [
        a_moved("render", "pub", Reached::FromProduction),
        a_moved("normalise", "pub(crate)", Reached::Nowhere),
    ];

    // When its named facade is written
    let facade = the_named_facade_of(&items);

    // Then the helper is not re-exported: it travelled with its only caller
    assert_eq!(facade, ["pub use rendering::{render};"]);
}

#[test]
fn a_named_facade_writes_names_only_tests_reach_under_cfg_test_after_the_production_lines() {
    // Given a seam whose items are reached from production, and one only from the parent's tests
    let items = [
        a_moved("render", "pub", Reached::FromProduction),
        a_moved("clamp", "pub(crate)", Reached::OnlyFromTests),
        a_moved("tier", "pub(crate)", Reached::FromProduction),
    ];

    // When its named facade is written
    let facade = the_named_facade_of(&items);

    // Then the production tiers come first and the test-only name follows under `#[cfg(test)]`
    assert_eq!(
        facade,
        [
            "pub use rendering::{render};",
            "pub(crate) use rendering::{tier};",
            "#[cfg(test)]",
            "pub(crate) use rendering::{clamp};",
        ]
    );
}

#[test]
fn a_pub_item_reached_only_from_tests_stays_on_the_pub_line() {
    // Given a `pub` item only the parent's tests reach
    let items = [a_moved("render", "pub", Reached::OnlyFromTests)];

    // When its named facade is written
    let facade = the_named_facade_of(&items);

    // Then its public path is interface, and is not gated for tests
    assert_eq!(facade, ["pub use rendering::{render};"]);
}

#[test]
fn a_named_facade_binds_every_name_it_writes_including_unreferenced_pub_and_test_only_names() {
    // Given a seam with an unreferenced `pub` item and a name only tests reach
    let items = [
        a_moved("statements", "pub", Reached::Nowhere),
        a_moved("clamp", "pub(crate)", Reached::OnlyFromTests),
    ];

    // When the parent asks whether the facade binds each name
    let binds_statements = facade_will_bind("statements", &items, Reexport::Named);
    let binds_clamp = facade_will_bind("clamp", &items, Reexport::Named);

    // Then both are bound, as the facade writes both
    assert_eq!((binds_statements, binds_clamp), (true, true));
}

#[test]
fn a_glob_facade_is_pub_when_anything_moved_is_pub_and_pub_crate_otherwise() {
    // Given one seam that moved a `pub` item and one that moved only narrower ones
    let with_a_pub_item = [
        a_moved("PreImage", "pub", Reached::Nowhere),
        a_moved("capture", "pub(crate)", Reached::FromProduction),
    ];
    let without_one = [a_moved("capture", "pub(crate)", Reached::FromProduction)];

    // When their glob facades are written
    let wide = facade_lines("group", &with_a_pub_item, Reexport::Glob).expect("a glob is written");
    let narrow = facade_lines("group", &without_one, Reexport::Glob).expect("a glob is written");

    // Then the width follows the widest item, whatever reaches it
    assert_eq!(
        (wide, narrow),
        (
            vec!["pub use group::*;".to_string()],
            vec!["pub(crate) use group::*;".to_string()]
        )
    );
}
