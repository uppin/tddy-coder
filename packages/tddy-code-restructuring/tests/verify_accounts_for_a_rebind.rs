//! `verify` accounts for a `self` the author declares rebound, and for nothing it was not told of.
//!
//! `read_fields_through` inserts `let state = <expr>;` before a range and writes every `self` the
//! range reads as `state` (dropping a `&` the state value already provides). That is not a
//! behaviour change, but `verify` reads it as a loss and a gain per statement plus a gained `let`.
//! It now takes a declaration, `--rebind NAME`, and excuses exactly what that rebind produces: a
//! rebind nobody declared, a second `let` and a changed argument stay reported.
//!
//! Library level: `verify::compare_with` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare, compare_with, Comparison, Declared, Rebind};

fn a_crate_holding(source: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("src/lib.rs".to_string(), source.to_string())])
}

/// What a declaration of `rebinds` (each a binding name) makes of `before` against `after`.
fn comparing_declared(before: &str, after: &str, rebinds: &[&str]) -> Comparison {
    let rebinds: Vec<String> = rebinds.iter().map(|text| (*text).to_string()).collect();
    let declared = Declared::default()
        .with_rebinds(&rebinds)
        .expect("the declaration reads");
    compare_with(&a_crate_holding(before), &a_crate_holding(after), &declared)
}

const A_METHOD_READING_ITS_FIELDS: &str = concat!(
    "impl Host {\n",
    "    fn total(&self) -> u32 {\n",
    "        let a = self.rosters.len() as u32;\n",
    "        limit_of(&self.config) + a\n",
    "    }\n",
    "}\n",
);
const THE_METHOD_READING_THROUGH_STATE: &str = concat!(
    "impl Host {\n",
    "    fn total(&self) -> u32 {\n",
    "        let state = self.state();\n",
    "        let a = state.rosters.len() as u32;\n",
    "        limit_of(state.config) + a\n",
    "    }\n",
    "}\n",
);

#[test]
fn a_declared_rebind_accounts_for_the_rebound_statements_the_dropped_borrow_and_its_one_let() {
    // Given a method whose tail now reads its fields through `state`, one borrow dropped
    let (before, after) = (
        A_METHOD_READING_ITS_FIELDS,
        THE_METHOD_READING_THROUGH_STATE,
    );

    // When it is compared with `state` declared
    let comparison = comparing_declared(before, after, &["state"]);

    // Then nothing is reported, and the two rebound statements are counted as re-pointed
    assert!(
        comparison.holds() && comparison.excused.repointed >= 2,
        "a declared rebind was not accounted for: {comparison:?}"
    );
}

#[test]
fn without_the_declaration_the_rebind_is_reported_and_with_it_a_second_let_or_a_changed_argument_still_is(
) {
    // Given the rebind undeclared, and two changes the declaration does not cover
    let undeclared = compare(
        &a_crate_holding(A_METHOD_READING_ITS_FIELDS),
        &a_crate_holding(THE_METHOD_READING_THROUGH_STATE),
    );
    let a_second_let = comparing_declared(
        A_METHOD_READING_ITS_FIELDS,
        &THE_METHOD_READING_THROUGH_STATE.replace(
            "        let state = self.state();\n",
            "        let state = self.state();\n        let state = self.state();\n",
        ),
        &["state"],
    );
    let a_changed_argument = comparing_declared(
        A_METHOD_READING_ITS_FIELDS,
        &THE_METHOD_READING_THROUGH_STATE
            .replace("state.rosters.len()", "state.rosters.capacity()"),
        &["state"],
    );

    // When each is compared
    // Then the undeclared rebind is a loss and a gain, and the second `let` and the changed call are reported
    assert!(
        !undeclared.holds()
            && undeclared
                .added
                .contains(&"let state = self.state();".to_string()),
        "an undeclared rebind was excused: {undeclared:?}"
    );
    assert_eq!(
        a_second_let.added,
        vec!["let state = self.state();".to_string()]
    );
    assert_eq!(
        (a_changed_argument.missing, a_changed_argument.added),
        (
            vec!["let a = self.rosters.len() as u32;".to_string()],
            vec!["let a = state.rosters.capacity() as u32;".to_string()]
        )
    );
}

#[test]
fn a_rebind_declaration_must_be_one_identifier_other_than_self() {
    // Given a binding name, and four texts that are not one
    let names = ["state", "self", "two words", "a.b", ""];

    // When each is read as a declaration
    let read: Vec<bool> = names
        .iter()
        .map(|name| name.parse::<Rebind>().is_ok())
        .collect();

    // Then only the identifier other than `self` is a rebind
    assert_eq!(read, [true, false, false, false, false]);
}
