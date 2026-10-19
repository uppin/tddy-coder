//! `verify --detach TYPE=NAME` accounts for what a `detach_method` run produced, and for nothing it
//! was not told of — `#reshape` 15/24, rule R7.
//!
//! A detach writes each member as `fn m(session: &mut Host, …)`, rebinds its body's `self` to
//! `session`, rewrites each caller `h.m(a)` as `m(h, a)` (or `host::m(h, a)`) and removes the block
//! it empties. None of that changes behaviour, and `verify` reads all of it as losses and gains. The
//! declaration excuses exactly those; a changed argument is still reported.
//!
//! Library level: `verify::compare_with` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::{compare_with, Comparison, Declared};

/// A crate whose `host` module holds `host` and whose `tick` module holds `tick`.
fn a_crate_holding(host: &str, tick: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("src/host.rs".to_string(), host.to_string()),
        ("src/tick.rs".to_string(), tick.to_string()),
    ])
}

/// What `verify --detach <detaches>` makes of `before` against `after`, each a `(host, tick)` pair.
fn verifying_with_detaches(
    before: (&str, &str),
    after: (&str, &str),
    detaches: &[&str],
) -> Comparison {
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
fn verify_holds_for_a_detach_run() {
    // Given a block whose two members are now functions over `session`, its `Self::step` a plain
    // call, its block removed, and the caller in `tick` re-pointed through the module
    let before = (A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD);
    let after = (THE_HOST_WITH_ITS_FUNCTIONS, THE_TICK_CALLING_THE_FUNCTION);

    // When it is verified with `--detach Host=session`
    let comparison = verifying_with_detaches(before, after, &["Host=session"]);

    // Then it holds, the signature, the calls and the rebound statements counted as re-pointed
    assert!(
        comparison.holds() && comparison.excused.repointed >= 4,
        "a declared detach was not accounted for: {comparison:?}"
    );
}

#[test]
fn verify_reports_a_changed_argument() {
    // Given the same detach, except that the re-pointed call passes `3` where the method got `2`
    let before = (A_HOST_WITH_ITS_METHODS, A_TICK_CALLING_THE_METHOD);
    let changed = THE_TICK_CALLING_THE_FUNCTION.replace("host::bump(h, 2)", "host::bump(h, 3)");
    let after = (THE_HOST_WITH_ITS_FUNCTIONS, changed.as_str());

    // When it is verified with `--detach Host=session`
    let comparison = verifying_with_detaches(before, after, &["Host=session"]);

    // Then the call is reported, lost and gained, and everything else is still accounted for
    assert_eq!(
        (comparison.missing, comparison.added),
        (
            vec!["h.bump(2)".to_string()],
            vec!["host::bump(h, 3)".to_string()]
        )
    );
}
