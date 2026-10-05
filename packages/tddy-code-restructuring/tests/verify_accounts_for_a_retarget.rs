//! `verify` accounts for an `impl` retarget the author declares, and for nothing it was not told of.
//!
//! A retarget changes the self type of an `impl` header and the `Old::` prefix of a path, and the
//! split of a block adds up to two headers. None of that is a behaviour change, and `verify` could
//! not tell it from a hand edit, so it reported every one as a loss. It now takes a declaration,
//! `OLD=NEW`, and excuses exactly the differences that declaration produces: a rename that was not
//! declared, a changed argument, a lost statement or a lost comment stay reported.
//!
//! Library level: `verify::compare_with` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare, compare_with, Comparison, Declared};

fn a_crate_holding(source: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("src/lib.rs".to_string(), source.to_string())])
}

/// What a declaration of `retargets` (each `OLD=NEW`) makes of `before` against `after`.
fn comparing_declared(before: &str, after: &str, retargets: &[&str]) -> Comparison {
    let retargets: Vec<String> = retargets.iter().map(|text| (*text).to_string()).collect();
    let declared = Declared::from_texts(&retargets).expect("the declaration reads");
    compare_with(&a_crate_holding(before), &a_crate_holding(after), &declared)
}

/// A block whose reader calls a function of the same block through the type: the shape in which a
/// retarget leaves a statement behind. (An `impl` header is not a statement `verify` reads, so the
/// self type changing is invisible to it; the `Host::` of a path is not.)
const A_HOST_WHOSE_READER_BUILDS: &str =
    "impl Host {\n    fn rebuilt(&self) -> Host {\n        Host::build(self.n)\n    }\n}\n";
const A_ROSTER_WHOSE_READER_BUILDS: &str =
    "impl Roster {\n    fn rebuilt(&self) -> Host {\n        Roster::build(self.n)\n    }\n}\n";

#[test]
fn a_whole_block_retarget_is_accounted_for_once_it_is_declared() {
    // Given a block retargeted to `Roster`, whose members' paths were re-pointed with it
    let (before, after) = (A_HOST_WHOSE_READER_BUILDS, A_ROSTER_WHOSE_READER_BUILDS);

    // When it is compared with `Host=Roster` declared
    let comparison = comparing_declared(before, after, &["Host=Roster"]);

    // Then nothing is reported, and the pairing is counted
    assert!(
        comparison.holds() && comparison.excused.repointed >= 1,
        "a declared retarget was not accounted for: {comparison:?}"
    );
}

#[test]
fn the_same_change_is_reported_when_no_retarget_is_declared() {
    // Given the same two texts
    let (before, after) = (A_HOST_WHOSE_READER_BUILDS, A_ROSTER_WHOSE_READER_BUILDS);

    // When they are compared with no declaration
    let comparison = compare(&a_crate_holding(before), &a_crate_holding(after));

    // Then the re-pointed call is a loss and a gain: `verify` never excuses a rename unprompted
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["Host::build(self.n)".to_string()],
            vec!["Roster::build(self.n)".to_string()]
        )
    );
}

const A_GENERIC_HOST_OF_THREE: &str = "impl<T> Host<T>\nwhere\n    T: Copy,\n{\n    fn get(&self) -> T {\n        self.v\n    }\n    fn put(&mut self, v: T) {\n        self.v = v;\n    }\n    fn size(&self) -> u32 {\n        1\n    }\n}\n";
const THE_SAME_SPLIT_AT_PUT: &str = "impl<T> Host<T>\nwhere\n    T: Copy,\n{\n    fn get(&self) -> T {\n        self.v\n    }\n}\nimpl<T> Roster<T>\nwhere\n    T: Copy,\n{\n    fn put(&mut self, v: T) {\n        self.v = v;\n    }\n}\nimpl<T> Host<T>\nwhere\n    T: Copy,\n{\n    fn size(&self) -> u32 {\n        1\n    }\n}\n";

#[test]
fn a_split_retarget_accounts_for_the_two_headers_it_adds() {
    // Given a generic block with a `where` clause, split at its middle member: two more headers,
    // each repeating the clause. (`impl Host {` is not a statement `verify` reads; `impl<T> Host<T>`
    // and the lines of a `where` clause are.)
    let (before, after) = (A_GENERIC_HOST_OF_THREE, THE_SAME_SPLIT_AT_PUT);

    // When it is compared with the retarget declared, and without
    let declared = comparing_declared(before, after, &["Host=Roster"]);
    let undeclared = compare(&a_crate_holding(before), &a_crate_holding(after));

    // Then the declaration accounts for both added headers, and without it both are reported
    assert!(
        declared.holds(),
        "the repeated clauses were reported: {declared:?}"
    );
    assert_eq!(
        undeclared.added,
        vec![
            "T: Copy,".to_string(),
            "T: Copy,".to_string(),
            "impl<T> Host<T>".to_string(),
            "impl<T> Roster<T>".to_string(),
            "where".to_string(),
            "where".to_string()
        ]
    );
}

#[test]
fn a_path_re_pointed_through_the_old_type_pairs_with_its_original() {
    // Given a call written through the old type that now names the new one
    let (before, after) = (
        "fn run() {\n    Host::build(1);\n}\n",
        "fn run() {\n    Roster::build(1);\n}\n",
    );

    // When it is compared with the retarget declared
    let comparison = comparing_declared(before, after, &["Host=Roster"]);

    // Then the pair is accounted for
    assert!(
        comparison.holds() && comparison.excused.repointed >= 1,
        "the re-pointed path was reported: {comparison:?}"
    );
}

#[test]
fn a_changed_argument_is_still_reported() {
    // Given a re-pointed call whose argument also changed
    let (before, after) = (
        "fn run() {\n    Host::build(1);\n}\n",
        "fn run() {\n    Roster::build(2);\n}\n",
    );

    // When it is compared with the retarget declared
    let comparison = comparing_declared(before, after, &["Host=Roster"]);

    // Then the changed statement is reported on both sides
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["Host::build(1);".to_string()],
            vec!["Roster::build(2);".to_string()]
        )
    );
}

#[test]
fn a_rename_to_a_type_other_than_the_declared_one_is_not_excused() {
    // Given a call re-pointed to a type the declaration does not name
    let (before, after) = (
        "fn run() {\n    Host::build(1);\n}\n",
        "fn run() {\n    Other::build(1);\n}\n",
    );

    // When it is compared with `Host=Roster` declared
    let comparison = comparing_declared(before, after, &["Host=Roster"]);

    // Then it is reported
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["Host::build(1);".to_string()],
            vec!["Other::build(1);".to_string()]
        )
    );
}

#[test]
fn a_declared_retarget_does_not_excuse_a_lost_comment() {
    // Given a retarget that also lost the banner above the block
    let before = format!("// banner: the readers\n{A_HOST_WHOSE_READER_BUILDS}");
    let after = A_ROSTER_WHOSE_READER_BUILDS;

    // When it is compared with the retarget declared
    let comparison = comparing_declared(&before, after, &["Host=Roster"]);

    // Then exactly the banner is reported
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["// banner: the readers".to_string()],
            Vec::<String>::new()
        )
    );
}

#[test]
fn a_declaration_is_two_different_bare_type_names() {
    // Given declarations that are not `OLD=NEW` of two different bare names
    let not_declarations = [
        "Host",
        "Host=",
        "=Roster",
        "Host=Host",
        "a::Host=Roster",
        "Host=Roster=Pair",
    ];

    // When each is read
    let outcomes = not_declarations.map(|text| Declared::from_texts(&[text.to_string()]).is_err());

    // Then every one is refused
    assert_eq!(outcomes, [true; 6]);
}

const A_HOST_WHOSE_READER_IS_FORWARDED: &str =
    "impl Host {\n    pub fn get(&self) -> u32 {\n        self.n\n    }\n}\n";
const THE_SAME_WITH_A_DELEGATOR_LEFT: &str = "impl Host {\n    pub fn get(&self) -> u32 {\n        self.roster().get()\n    }\n}\nimpl Roster {\n    pub fn get(&self) -> u32 {\n        self.n\n    }\n}\n";

/// The declaration of a delegator is the retarget it comes with; if the delegator milestone gives it
/// a carrier of its own (`VerifyRequest` field 4), this test's declaration line changes with it.
#[test]
fn a_declared_delegator_is_accounted_for_and_an_undeclared_forwarding_method_is_reported() {
    // Given a block retargeted with a forwarding method left on the old type
    let (before, after) = (
        A_HOST_WHOSE_READER_IS_FORWARDED,
        THE_SAME_WITH_A_DELEGATOR_LEFT,
    );

    // When it is compared with the retarget declared, and without
    let declared = comparing_declared(before, after, &["Host=Roster"]);
    let undeclared = compare(&a_crate_holding(before), &a_crate_holding(after));

    // Then the forwarding method and its duplicated signature are accounted for, and without a declaration both are reported
    assert!(declared.holds(), "the delegator was reported: {declared:?}");
    assert_eq!(
        undeclared.added,
        vec![
            "pub fn get(&self) -> u32 {".to_string(),
            "self.roster().get()".to_string()
        ]
    );
}
