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

const A_GENERIC_PAIR_OF_TWO_MEMBERS: &str = concat!(
    "pub struct Pair<T> {\n",
    "    a: T,\n",
    "}\n",
    "\n",
    "impl<T: Clone> Pair<T>\n",
    "where\n",
    "    T: Default,\n",
    "{\n",
    "    pub fn first(&self) -> T {\n",
    "        self.a.clone()\n",
    "    }\n",
    "\n",
    "    pub fn reset(&mut self) {\n",
    "        self.a = T::default();\n",
    "    }\n",
    "}\n",
);
const THE_PAIR_AFTER_RESET_LEFT: &str = concat!(
    "pub struct Pair<T> {\n",
    "    a: T,\n",
    "}\n",
    "\n",
    "impl<T: Clone> Pair<T>\n",
    "where\n",
    "    T: Default,\n",
    "{\n",
    "    pub fn first(&self) -> T {\n",
    "        self.a.clone()\n",
    "    }\n",
    "}\n",
);
const A_MODULE_HOLDING_RESET_IN_A_NEW_BLOCK: &str = concat!(
    "use super::Pair;\n",
    "\n",
    "impl<T: Clone> Pair<T>\n",
    "where\n",
    "    T: Default,\n",
    "{\n",
    "    pub fn reset(&mut self) {\n",
    "        self.a = T::default();\n",
    "    }\n",
    "}\n",
);

fn a_pair_crate(pair: &str, extra: Option<(&str, &str)>) -> BTreeMap<String, String> {
    let mut files = BTreeMap::from([
        ("src/lib.rs".to_string(), "pub mod pair;\n".to_string()),
        ("src/pair.rs".to_string(), pair.to_string()),
    ]);
    if let Some((path, text)) = extra {
        files.insert(path.to_string(), text.to_string());
    }
    files
}

#[test]
fn a_repeated_generic_header_of_a_member_run_is_no_difference() {
    // Given `reset` moved out of `impl<T: Clone> Pair<T> where T: Default` into a new block that
    // repeats the header
    let before = a_pair_crate(A_GENERIC_PAIR_OF_TWO_MEMBERS, None);
    let after = a_pair_crate(
        THE_PAIR_AFTER_RESET_LEFT,
        Some(("src/roster.rs", A_MODULE_HOLDING_RESET_IN_A_NEW_BLOCK)),
    );

    // When the two trees are compared
    let comparison = compare(&before, &after);

    // Then the repeated header is excused, and nothing is reported
    assert!(
        comparison.holds(),
        "the repeated header was reported: {comparison:?}"
    );
}

#[test]
fn a_gained_where_clause_without_the_whole_header_is_still_reported() {
    // Given the same tree, with a `where` clause and a predicate added to an unrelated block
    let before = a_pair_crate(A_GENERIC_PAIR_OF_TWO_MEMBERS, None);
    let stray = "fn stray<T>(value: T)\nwhere\n    T: Default,\n{\n    drop(value);\n}\n";
    let after = a_pair_crate(
        A_GENERIC_PAIR_OF_TWO_MEMBERS,
        Some(("src/roster.rs", stray)),
    );

    // When the two trees are compared
    let comparison = compare(&before, &after);

    // Then the header lines are not excused: a hand edit is not a repeated header
    assert!(
        comparison.added.contains(&"where".to_string())
            && comparison.added.contains(&"T: Default,".to_string()),
        "a stray `where` clause was excused: {comparison:?}"
    );
}
