//! `verify` accounts for methods the author declares detached, and for nothing it was not told of.
//!
//! `detach_method` writes a member as `fn m(session: &mut Host, …)`, rebinds its body's `self` to
//! `session`, rewrites each caller `h.m(a)` as `m(h, a)` (or `host::m(h, a)`) and removes a block it
//! empties. None of that changes behaviour, but `verify` reads it as losses and gains. The
//! declaration `--detach Host=session` excuses exactly what a detach produces; a detach nobody
//! declared, a changed argument and a receiver changed other than by the borrow stay reported.
//!
//! Library level: `verify::compare_with` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare, compare_with, Comparison, Declared, Detach};

fn a_crate_holding(host: &str, tick: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("src/host.rs".to_string(), host.to_string()),
        ("src/tick.rs".to_string(), tick.to_string()),
    ])
}

/// What a declaration of `detaches` (each `TYPE=NAME`) makes of `before` against `after`, each a
/// `(host, tick)` pair.
fn comparing_declared(before: (&str, &str), after: (&str, &str), detaches: &[&str]) -> Comparison {
    let detaches: Vec<String> = detaches.iter().map(|text| (*text).to_string()).collect();
    let declared = Declared::default()
        .with_detaches(&detaches)
        .expect("the declaration reads");
    compare_with(
        &a_crate_holding(before.0, before.1),
        &a_crate_holding(after.0, after.1),
        &declared,
    )
}

const A_HOST_WITH_ITS_METHODS: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(super) fn bump(&mut self, by: u32) -> u32 {\n",
    "        self.n += Self::step(by);\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    fn step(by: u32) -> u32 {\n",
    "        by * 2\n",
    "    }\n",
    "}\n",
);
const THE_HOST_WITH_ITS_FUNCTIONS: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "pub(super) fn bump(session: &mut Host, by: u32) -> u32 {\n",
    "    session.n += step(by);\n",
    "    session.n\n",
    "}\n",
    "\n",
    "fn step(by: u32) -> u32 {\n",
    "    by * 2\n",
    "}\n",
);
const A_TICK_CALLING_THE_METHOD: &str =
    "use crate::host::Host;\n\npub fn tick(h: &mut Host) -> u32 {\n    h.bump(2)\n}\n";
const THE_TICK_CALLING_THE_FUNCTION: &str = "use crate::host;\nuse crate::host::Host;\n\npub fn tick(h: &mut Host) -> u32 {\n    host::bump(h, 2)\n}\n";

#[test]
fn a_declared_detach_accounts_for_the_signatures_the_calls_the_rebound_bodies_and_the_emptied_block(
) {
    // Given a block whose two members are now functions over `session`, and a caller re-pointed
    let before = (A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD);
    let after = (THE_HOST_WITH_ITS_FUNCTIONS, THE_TICK_CALLING_THE_FUNCTION);

    // When it is compared with `Host=session` declared
    let comparison = comparing_declared(before, after, &["Host=session"]);

    // Then nothing is reported, and the signature, the call and the rebound statements are counted
    assert!(
        comparison.holds() && comparison.excused.repointed >= 4,
        "a declared detach was not accounted for: {comparison:?}"
    );
}

#[test]
fn without_the_declaration_the_detach_is_reported_and_with_it_a_changed_argument_or_receiver_still_is(
) {
    // Given the detach undeclared, and two changes the declaration does not cover
    let undeclared = compare(
        &a_crate_holding(A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD),
        &a_crate_holding(THE_HOST_WITH_ITS_FUNCTIONS, THE_TICK_CALLING_THE_FUNCTION),
    );
    let a_changed_argument = comparing_declared(
        (A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD),
        (
            THE_HOST_WITH_ITS_FUNCTIONS,
            &THE_TICK_CALLING_THE_FUNCTION.replace("host::bump(h, 2)", "host::bump(h, 3)"),
        ),
        &["Host=session"],
    );
    let a_changed_receiver = comparing_declared(
        (A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD),
        (
            THE_HOST_WITH_ITS_FUNCTIONS,
            &THE_TICK_CALLING_THE_FUNCTION.replace("host::bump(h, 2)", "host::bump(other, 2)"),
        ),
        &["Host=session"],
    );

    // When each is compared
    // Then the undeclared detach is reported, and so are the changed argument and receiver
    assert!(
        !undeclared.holds() && undeclared.added.contains(&"host::bump(h, 2)".to_string()),
        "an undeclared detach was excused: {undeclared:?}"
    );
    assert_eq!(
        (a_changed_argument.missing, a_changed_argument.added),
        (
            vec!["h.bump(2)".to_string()],
            vec!["host::bump(h, 3)".to_string()]
        )
    );
    assert_eq!(
        (a_changed_receiver.missing, a_changed_receiver.added),
        (
            vec!["h.bump(2)".to_string()],
            vec!["host::bump(other, 2)".to_string()]
        )
    );
}

#[test]
fn a_detach_declaration_names_one_type_and_one_identifier_other_than_self() {
    // Given a declaration, and five texts that are not one
    let texts = [
        "Host=session",
        "Host=self",
        "Host",
        "a::Host=session",
        "Host=two words",
        "=session",
    ];

    // When each is read
    let read: Vec<bool> = texts
        .iter()
        .map(|text| text.parse::<Detach>().is_ok())
        .collect();

    // Then only a type segment and an identifier other than `self` is a detach
    assert_eq!(read, [true, false, false, false, false, false]);
}
