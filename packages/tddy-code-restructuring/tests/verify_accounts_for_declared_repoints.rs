//! `verify` accounts for a call re-point the author declares, and for nothing it was not told of.
//!
//! `repoint_call` changes the part of a call in front of its argument list: `self.slot(x)` becomes
//! `self.peer.slot(x)`. That is not a behaviour change, but `verify` reads it as one lost and one
//! gained statement per call, and could not tell it from a hand edit. It now takes a declaration,
//! `OLD=NEW` callee texts, and excuses exactly the pairs that declaration produces: a hop nobody
//! declared, a replaced hop, a changed argument and a different method stay reported.
//!
//! What `verify` does *not* need told, and this suite does not pin: a call re-pointed to a lowercase
//! module (`f(` becoming `m::f(`) is already excused by the re-point pass with no declaration. What
//! it does not excuse is a hop on a receiver, a method chain, or a path qualified by a type.
//!
//! Library level: `verify::compare_with` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare, compare_with, Comparison, Declared};

fn a_crate_holding(source: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("src/lib.rs".to_string(), source.to_string())])
}

/// What a declaration of `repoints` (each `OLD=NEW`) makes of `before` against `after`.
fn comparing_declared(before: &str, after: &str, repoints: &[&str]) -> Comparison {
    let repoints: Vec<String> = repoints.iter().map(|text| (*text).to_string()).collect();
    let declared = Declared::from_declarations(&[], &repoints).expect("the declaration reads");
    compare_with(&a_crate_holding(before), &a_crate_holding(after), &declared)
}

const A_CALL_ON_THE_HOST: &str =
    "fn run(h: &Host) -> u32 {\n    let total = h.slot(1);\n    total\n}\n";
const THE_CALL_THROUGH_A_PEER: &str =
    "fn run(h: &Host) -> u32 {\n    let total = h.peer.slot(1);\n    total\n}\n";

#[test]
fn a_declared_receiver_hop_is_accounted_for_and_counted_as_repointed() {
    // Given a call whose receiver gained a hop
    let (before, after) = (A_CALL_ON_THE_HOST, THE_CALL_THROUGH_A_PEER);

    // When it is compared with the re-point declared, and without
    let declared = comparing_declared(before, after, &[".slot=.peer.slot"]);
    let undeclared = compare(&a_crate_holding(before), &a_crate_holding(after));

    // Then the declaration accounts for it and counts the pairing, and without it the call is a loss and a gain
    assert!(
        declared.holds() && declared.excused.repointed >= 1,
        "a declared re-point was not accounted for: {declared:?}"
    );
    assert_eq!(
        (undeclared.missing, undeclared.added),
        (
            vec!["let total = h.slot(1);".to_string()],
            vec!["let total = h.peer.slot(1);".to_string()]
        )
    );
}

#[test]
fn an_undeclared_hop_a_replaced_hop_and_a_changed_argument_are_still_reported() {
    // Given the declared re-point, and three changes it does not cover: another hop, a hop replaced
    // by a different one, and a changed argument besides the hop
    let declaration = [".slot=.peer.slot"];
    let another_hop = comparing_declared(
        A_CALL_ON_THE_HOST,
        "fn run(h: &Host) -> u32 {\n    let total = h.roster.slot(1);\n    total\n}\n",
        &declaration,
    );
    let a_replaced_hop = comparing_declared(
        "fn run(h: &Host) -> u32 {\n    let total = h.old.slot(1);\n    total\n}\n",
        THE_CALL_THROUGH_A_PEER,
        &declaration,
    );
    let a_changed_argument = comparing_declared(
        A_CALL_ON_THE_HOST,
        "fn run(h: &Host) -> u32 {\n    let total = h.peer.slot(2);\n    total\n}\n",
        &declaration,
    );

    // When each is compared
    // Then the statement is reported on both sides in every case
    assert_eq!(
        (another_hop.missing, another_hop.added),
        (
            vec!["let total = h.slot(1);".to_string()],
            vec!["let total = h.roster.slot(1);".to_string()]
        )
    );
    assert_eq!(
        (a_replaced_hop.missing, a_replaced_hop.added),
        (
            vec!["let total = h.old.slot(1);".to_string()],
            vec!["let total = h.peer.slot(1);".to_string()]
        )
    );
    assert_eq!(
        (a_changed_argument.missing, a_changed_argument.added),
        (
            vec!["let total = h.slot(1);".to_string()],
            vec!["let total = h.peer.slot(2);".to_string()]
        )
    );
}

#[test]
fn a_declared_callee_text_never_pairs_a_different_method() {
    // Given `.slot` declared, and a call of `.slots` that gained the same hop
    let comparison = comparing_declared(
        "fn run(h: &Host) -> u32 {\n    let total = h.slots(1);\n    total\n}\n",
        "fn run(h: &Host) -> u32 {\n    let total = h.peer.slots(1);\n    total\n}\n",
        &[".slot=.peer.slot"],
    );

    // When it is compared
    // Then the call is reported: `.slot` followed by `(` is the only place the declaration reads
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["let total = h.slots(1);".to_string()],
            vec!["let total = h.peer.slots(1);".to_string()]
        )
    );
}
