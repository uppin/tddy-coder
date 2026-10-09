//! `verify` holds for a run of `impl` members moved to another file, and still reports a loss
//! inside the moved run.
//!
//! A member move copies the members' bytes and authors only an `impl Host {` line, its closing brace,
//! `use` lines and a widened visibility. None of those is a difference `verify` reports: an `impl`
//! header of a plain type is not a statement it reads, `use` items are scaffolding, and a leading
//! `pub…` is stripped. So the move needs no declaration — and a statement dropped on the way is
//! still a loss.
//!
//! Library level: `verify::compare` over two sets of sources, no tree, no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::verify::compare;

const A_HOST_OF_TWO_MEMBERS: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn tick(&mut self) {\n",
    "        self.bump();\n",
    "    }\n",
    "\n",
    "    fn bump(&mut self) {\n",
    "        self.n += 1;\n",
    "    }\n",
    "}\n",
);
const THE_HOST_AFTER_BUMP_LEFT: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn tick(&mut self) {\n",
    "        self.bump();\n",
    "    }\n",
    "}\n",
);
const A_COUNTING_MODULE_HOLDING_BUMP: &str = concat!(
    "use super::Host;\n",
    "\n",
    "impl Host {\n",
    "    pub(super) fn bump(&mut self) {\n",
    "        self.n += 1;\n",
    "    }\n",
    "}\n",
);
const A_COUNTING_MODULE_HOLDING_A_HOLLOW_BUMP: &str = concat!(
    "use super::Host;\n",
    "\n",
    "impl Host {\n",
    "    pub(super) fn bump(&mut self) {\n",
    "    }\n",
    "}\n",
);

fn before_the_move() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("src/lib.rs".to_string(), "pub mod host;\n".to_string()),
        ("src/host.rs".to_string(), A_HOST_OF_TWO_MEMBERS.to_string()),
    ])
}

fn after_the_move_with(counting: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("src/lib.rs".to_string(), "pub mod host;\n".to_string()),
        (
            "src/host.rs".to_string(),
            format!("mod counting;\n\n{THE_HOST_AFTER_BUMP_LEFT}"),
        ),
        ("src/host/counting.rs".to_string(), counting.to_string()),
    ])
}

#[test]
fn a_member_run_moved_to_another_file_is_no_difference() {
    // Given `bump` moved out of `host` into a new block of `host::counting`, widened to `pub(super)`
    let (before, after) = (
        before_the_move(),
        after_the_move_with(A_COUNTING_MODULE_HOLDING_BUMP),
    );

    // When the two trees are compared
    let comparison = compare(&before, &after);

    // Then nothing is reported
    assert!(
        comparison.holds(),
        "the member move was reported: {comparison:?}"
    );
}

#[test]
fn a_dropped_statement_in_the_moved_run_is_still_reported() {
    // Given the same move, with `bump`'s statement lost on the way
    let (before, after) = (
        before_the_move(),
        after_the_move_with(A_COUNTING_MODULE_HOLDING_A_HOLLOW_BUMP),
    );

    // When the two trees are compared
    let comparison = compare(&before, &after);

    // Then the lost statement is reported
    assert_eq!(comparison.missing, vec!["self.n += 1;".to_string()]);
}
