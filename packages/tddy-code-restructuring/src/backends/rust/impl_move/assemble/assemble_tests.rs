//! The assembly of a member move, read as a function of texts: no server, no workspace.

use serde_json::json;

use super::super::super::item_move::destination::Module;
use super::super::super::item_move::members::MemberWidening;
use super::super::super::item_move::outline::Item;
use super::super::super::retarget_impl::outline::{Member, Run};
use super::{assemble, Assembled, MovingMembers};

const ORIGIN: &str = "src/host.rs";
const DESTINATION: &str = "src/roster.rs";

/// `impl Host` with four members: `new` (lines 6–8), `bump` (10–12), `twice` (14–17), `n` (19–21).
const A_HOST_OF_FOUR_MEMBERS: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn new() -> Host {\n",
    "        Host { n: 0 }\n",
    "    }\n",
    "\n",
    "    fn bump(&mut self) {\n",
    "        self.n += 1;\n",
    "    }\n",
    "\n",
    "    fn twice(&mut self) {\n",
    "        self.bump();\n",
    "        self.bump();\n",
    "    }\n",
    "\n",
    "    pub fn n(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
);

/// The block `bump` and `twice` arrive as, when the destination has no block of `Host`.
const BUMP_AND_TWICE_AS_A_NEW_BLOCK: &str = concat!(
    "impl Host {\n",
    "    fn bump(&mut self) {\n",
    "        self.n += 1;\n",
    "    }\n",
    "\n",
    "    fn twice(&mut self) {\n",
    "        self.bump();\n",
    "        self.bump();\n",
    "    }\n",
    "}\n",
);

/// A documented `impl Host` holding `bump` alone (lines 7–9), followed by a free function.
const A_HOST_WHOSE_DOCUMENTED_BLOCK_HOLDS_ONE_MEMBER: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "/// Counting.\n",
    "impl Host {\n",
    "    fn bump(&mut self) {\n",
    "        self.n += 1;\n",
    "    }\n",
    "}\n",
    "\n",
    "pub fn free() {}\n",
);

/// A generic block with a `where` clause: `first` (lines 9–11) stays, `reset` (13–15) moves.
const A_GENERIC_PAIR: &str = concat!(
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
    "    fn reset(&mut self) {\n",
    "        self.a = T::default();\n",
    "    }\n",
    "}\n",
);

const A_ROSTER_WITH_A_TEST_MODULE: &str = "pub fn roster() {}\n\n#[cfg(test)]\nmod tests {}\n";
const A_ROSTER_WITH_ONE_HOST_BLOCK: &str =
    "use crate::host::Host;\n\nimpl Host {\n    fn other(&self) {}\n}\n";
const A_ROSTER_WITH_TWO_HOST_BLOCKS: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "impl Host {\n",
    "    fn other(&self) {}\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    fn another(&self) {}\n",
    "}\n",
);

/// A member named `name` on lines `first..=last`, its name reported on `first`.
fn a_member(name: &str, first: u32, last: u32) -> Member {
    Member {
        name: name.to_string(),
        position: json!({ "line": first - 1, "character": 7 }),
        first_line: first,
        last_line: last,
    }
}

/// The run of `members[moved]` of the block of `self_type` on lines `header..=close`.
fn a_run_of(
    members: Vec<Member>,
    moved: std::ops::Range<usize>,
    self_type: &str,
    lines: std::ops::RangeInclusive<u32>,
) -> Run {
    Run {
        members,
        first_moved: moved.start,
        moved_end: moved.end,
        self_type: self_type.to_string(),
        header_line: *lines.start(),
        close_line: *lines.end(),
    }
}

fn the_four_members() -> Vec<Member> {
    vec![
        a_member("new", 6, 8),
        a_member("bump", 10, 12),
        a_member("twice", 14, 17),
        a_member("n", 19, 21),
    ]
}

/// The `roster` module spanning the whole of `text`.
fn the_roster_module_of(text: &str) -> Module {
    Module {
        file: DESTINATION.to_string(),
        scope: 0..text.len(),
        path: vec!["roster".to_string()],
    }
}

/// The `Host` struct, as a root item of `host` the moved members name.
fn the_host_struct() -> Item {
    Item {
        name: "Host".to_string(),
        position: json!({ "line": 0, "character": 11 }),
        visibility: "pub".to_string(),
    }
}

/// Assemble moving `run` of `origin_text` into the `roster` module holding `destination_text`.
fn moving_into_the_roster(origin_text: &str, run: &Run, destination_text: &str) -> Assembled {
    let source = vec!["host".to_string()];
    let destination = the_roster_module_of(destination_text);
    let reached = vec![the_host_struct()];
    let widening = MemberWidening::default();
    assemble(&MovingMembers {
        source_file: ORIGIN,
        source_text: origin_text,
        source: &source,
        run,
        destination: &destination,
        destination_text,
        created: None,
        reached: &reached,
        widening: &widening,
    })
    .expect("the member move assembles")
}

fn the_new_text_of<'a>(assembled: &'a Assembled, file: &str) -> &'a str {
    &assembled
        .files
        .get(file)
        .unwrap_or_else(|| panic!("the move changes {file}"))
        .1
}

#[test]
fn cuts_the_run_and_leaves_the_members_around_it_in_one_block() {
    // Given `bump` and `twice` move out of an `impl Host` of four members
    let run = a_run_of(the_four_members(), 1..3, "Host", 5..=22);

    // When the move is assembled
    let assembled =
        moving_into_the_roster(A_HOST_OF_FOUR_MEMBERS, &run, A_ROSTER_WITH_A_TEST_MODULE);

    // Then the origin keeps `new` and `n` in the one block, with one blank line between them
    assert_eq!(
        the_new_text_of(&assembled, ORIGIN),
        concat!(
            "pub struct Host {\n",
            "    n: u32,\n",
            "}\n",
            "\n",
            "impl Host {\n",
            "    pub fn new() -> Host {\n",
            "        Host { n: 0 }\n",
            "    }\n",
            "\n",
            "    pub fn n(&self) -> u32 {\n",
            "        self.n\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn removes_a_block_the_run_empties_with_its_doc_comment() {
    // Given the only member of a documented block moves
    let run = a_run_of(vec![a_member("bump", 7, 9)], 0..1, "Host", 6..=10);

    // When the move is assembled
    let assembled = moving_into_the_roster(
        A_HOST_WHOSE_DOCUMENTED_BLOCK_HOLDS_ONE_MEMBER,
        &run,
        A_ROSTER_WITH_A_TEST_MODULE,
    );

    // Then the block, its doc comment and the blank line after it are gone
    assert_eq!(
        the_new_text_of(&assembled, ORIGIN),
        "pub struct Host {\n    n: u32,\n}\n\npub fn free() {}\n"
    );
}

#[test]
fn opens_a_new_block_below_the_last_item_and_above_the_test_module() {
    // Given a destination holding no block of `Host`, and a trailing test module
    let run = a_run_of(the_four_members(), 1..3, "Host", 5..=22);

    // When the move is assembled
    let assembled =
        moving_into_the_roster(A_HOST_OF_FOUR_MEMBERS, &run, A_ROSTER_WITH_A_TEST_MODULE);

    // Then a new `impl Host` holds both members, between `roster` and the test module, and the
    // type is imported
    let roster = the_new_text_of(&assembled, DESTINATION);
    let block = roster
        .find(BUMP_AND_TWICE_AS_A_NEW_BLOCK)
        .unwrap_or_else(|| panic!("no new block holding `bump` and `twice`:\n{roster}"));
    let roster_fn = roster.find("pub fn roster() {}").expect("`roster` is kept");
    let tests = roster
        .find("#[cfg(test)]")
        .expect("the test module is kept");
    assert!(
        roster_fn < block && block < tests,
        "the new block is not between the last item and the test module:\n{roster}"
    );
    assert!(
        roster.contains("use crate::host::Host;"),
        "the destination does not import the block's type:\n{roster}"
    );
}

#[test]
fn joins_the_one_block_with_the_same_header() {
    // Given a destination holding exactly one `impl Host`
    let run = a_run_of(the_four_members(), 1..3, "Host", 5..=22);

    // When the move is assembled
    let assembled =
        moving_into_the_roster(A_HOST_OF_FOUR_MEMBERS, &run, A_ROSTER_WITH_ONE_HOST_BLOCK);

    // Then the members are appended to that block, after a blank line, and no second block opens
    let roster = the_new_text_of(&assembled, DESTINATION);
    assert!(
        roster.contains(concat!(
            "impl Host {\n",
            "    fn other(&self) {}\n",
            "\n",
            "    fn bump(&mut self) {\n",
            "        self.n += 1;\n",
            "    }\n",
            "\n",
            "    fn twice(&mut self) {\n",
            "        self.bump();\n",
            "        self.bump();\n",
            "    }\n",
            "}\n",
        )),
        "the members did not join the existing block:\n{roster}"
    );
    assert_eq!(
        roster.matches("impl Host {").count(),
        1,
        "a second block was opened:\n{roster}"
    );
}

#[test]
fn opens_a_new_block_when_two_blocks_share_the_header() {
    // Given a destination holding two blocks of `impl Host`
    let run = a_run_of(the_four_members(), 1..3, "Host", 5..=22);

    // When the move is assembled
    let assembled =
        moving_into_the_roster(A_HOST_OF_FOUR_MEMBERS, &run, A_ROSTER_WITH_TWO_HOST_BLOCKS);

    // Then neither is joined: a third block holds the members, and the two are as they were
    let roster = the_new_text_of(&assembled, DESTINATION);
    assert!(
        roster.contains(BUMP_AND_TWICE_AS_A_NEW_BLOCK),
        "no new block holds the members:\n{roster}"
    );
    assert!(
        roster.contains("impl Host {\n    fn other(&self) {}\n}\n")
            && roster.contains("impl Host {\n    fn another(&self) {}\n}\n"),
        "an existing block was changed:\n{roster}"
    );
}

#[test]
fn keeps_a_generic_header_and_its_where_clause() {
    // Given `reset` moves out of `impl<T: Clone> Pair<T> where T: Default`
    let run = a_run_of(
        vec![a_member("first", 9, 11), a_member("reset", 13, 15)],
        1..2,
        "Pair",
        5..=16,
    );

    // When the move is assembled
    let assembled = moving_into_the_roster(A_GENERIC_PAIR, &run, A_ROSTER_WITH_A_TEST_MODULE);

    // Then the new block repeats the header as written
    let roster = the_new_text_of(&assembled, DESTINATION);
    assert!(
        roster.contains(concat!(
            "impl<T: Clone> Pair<T>\n",
            "where\n",
            "    T: Default,\n",
            "{\n",
            "    fn reset(&mut self) {\n",
            "        self.a = T::default();\n",
            "    }\n",
            "}\n",
        )),
        "the header did not arrive as written:\n{roster}"
    );
}
