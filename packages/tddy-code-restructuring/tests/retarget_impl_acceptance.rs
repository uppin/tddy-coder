//! `retarget_impl` moves members of an inherent `impl` to another type of the same crate, against a
//! live rust-analyzer.
//!
//! Moving 22 methods from one type's `impl` to another's was a header edit per file and a `use`,
//! and no operation did it: the engine's guarantees (the compile gate, comments kept, paths
//! re-pointed from the server's own reference set) did not reach the hand edit.
//!
//! `cargo check` is the assertion no edit that merely looks right can satisfy: a header with the
//! wrong self type, a path left naming the old type or a `use` missing all fail it. A caller left
//! pointing at the old type fails it too, and that is the documented outcome, not a defect.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    applying_a_plan_of, assert_compiles, checking_the_plan, what_the_server_answers,
    AFixtureWorkspace,
};
use same_crate::{an_app_holding, blocks, the_anchor_over, the_impl_blocks_of};
use tddy_code_restructuring::{Anchor, RefactorOp};

const LIB: &str = "pub mod host;\npub mod roster;\n";
const A_ROSTER: &str = "pub struct Roster {\n    pub(crate) n: u32,\n}\n";
const A_HOST_STRUCT: &str = "pub struct Host {\n    pub(crate) n: u32,\n}\n\n";

/// The body of an `impl Host` the whole-block tests retarget: a doc line, an attribute and two kinds
/// of comment on the members it holds.
const COMMENTED_MEMBERS: &str = concat!(
    "    // banner: the readers\n",
    "    /// Read the count.\n",
    "    #[must_use]\n",
    "    pub fn get(&self) -> u32 {\n",
    "        // the count lives here\n",
    "        self.n\n",
    "    }\n",
);

/// The operation: retarget the members `anchor` names to the type `to_type`.
fn a_retarget_op(anchor: &Anchor, to_type: &str) -> RefactorOp {
    let op = serde_json::json!({ "op": "retarget_impl", "anchor": anchor, "to_type": to_type });
    serde_json::from_value(op).expect("a `retarget_impl` operation parses")
}

/// A crate whose `host` module holds `host_body` after `impl Host {`'s own struct, and whose
/// `roster` module declares `Roster { n }`.
fn a_crate_whose_host_reads(host_text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", host_text),
        ("src/roster.rs", A_ROSTER),
    ])
}

fn a_host_impl_of(members: &str) -> String {
    format!("{A_HOST_STRUCT}impl Host {{\n{members}}}\n")
}

/// The anchor the `anchors` command emits over `members`, each written as the `host` module's path
/// to it: `Host::put`, or `<Host>` for the whole block.
async fn the_anchor_over_members_of_host(
    workspace: &AFixtureWorkspace,
    members: &[&str],
) -> Anchor {
    let paths: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    the_anchor_over(workspace, "src/host.rs", &paths).await
}

/// A plan retargeting `members` of `src/host.rs` (all of the block when `members` is `<Host>`).
async fn retargeting(
    workspace: &AFixtureWorkspace,
    members: &[&str],
    to_type: &str,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let anchor = the_anchor_over_members_of_host(workspace, members).await;
    applying_a_plan_of(workspace, &[a_retarget_op(&anchor, to_type)]).await
}

/// The zero-based `(line, character)` of `name` on the first line of `text` holding `declaration`.
fn where_the_name_sits(text: &str, declaration: &str, name: &str) -> (usize, usize) {
    let (line, held) = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(declaration))
        .expect("the declaration is in the text");
    (line, held.find(name).expect("the name is on its line"))
}

fn the_line_holding(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| line.contains(needle))
        .expect("the needle is in the text")
}

const A_HOST_WITH_AN_ASSOCIATED_FUNCTION: &str = concat!(
    "pub struct Host {\n    pub(crate) n: u32,\n}\n\n",
    "impl Host {\n",
    "    pub fn build(n: u32) -> Host {\n",
    "        Host { n }\n",
    "    }\n",
    "\n",
    "    pub fn rebuilt(&self) -> Host {\n",
    "        Host::build(self.n)\n",
    "    }\n",
    "}\n",
);

/// M0 — the three premises the plan rests on, asked of the server itself: an `impl` is one outline
/// item of kind 19 whose members are its children, and `textDocument/references` at an associated
/// function reaches the `Host::build(..)` that names it inside the same `impl`.
///
/// The third premise, the hunks `minimal_edits` returns for a split, is a pure function of two
/// texts and is pinned by a unit test beside the module that calls it.
#[tokio::test(flavor = "multi_thread")]
async fn probe_the_outline_the_references_and_the_hunks_of_a_split() {
    // Given an `impl Host` holding an associated function that a method of the same block calls
    let workspace = a_crate_whose_host_reads(A_HOST_WITH_AN_ASSOCIATED_FUNCTION);
    let host = workspace.read("src/host.rs");
    let uri = format!(
        "file://{}",
        workspace
            .path()
            .join("src/host.rs")
            .canonicalize()
            .expect("a path")
            .display()
    );
    let (line, character) = where_the_name_sits(&host, "pub fn build", "build");

    // When the server is asked for the outline and for the references at `build`
    let answers = what_the_server_answers(
        &workspace,
        &[
            (
                "textDocument/documentSymbol",
                serde_json::json!({ "textDocument": { "uri": uri } }),
            ),
            (
                "textDocument/references",
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character },
                    "context": { "includeDeclaration": false },
                }),
            ),
        ],
    )
    .await;

    // Then the impl is one outline item of kind 19 holding both members as children
    let outline = answers[0].as_array().expect("an outline");
    let the_impl = outline
        .iter()
        .find(|symbol| symbol["kind"] == 19)
        .expect("the outline holds an item of kind 19");
    let members: Vec<&str> = the_impl["children"]
        .as_array()
        .expect("the impl has children")
        .iter()
        .map(|child| child["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        (the_impl["name"].as_str(), members),
        (Some("impl Host"), vec!["build", "rebuilt"]),
        "the outline of an impl is not the shape the operation reads: {outline:?}"
    );

    // And the references include the call written `Host::build(..)` inside the same impl
    let called_on = the_line_holding(&host, "Host::build(self.n)");
    let referenced_lines: Vec<u64> = answers[1]
        .as_array()
        .expect("references")
        .iter()
        .map(|site| site["range"]["start"]["line"].as_u64().expect("a line"))
        .collect();
    assert!(
        referenced_lines.contains(&(called_on as u64)),
        "the references {referenced_lines:?} do not reach line {called_on}, `Host::build(self.n)`"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn retargets_a_whole_impl_block_to_another_type_and_the_tree_compiles() {
    // Given an `impl Host` of one reader
    let workspace = a_crate_whose_host_reads(&a_host_impl_of(COMMENTED_MEMBERS));

    // When the whole block is retargeted to `Roster`
    retargeting(&workspace, &["<Host>"], "app::roster::Roster")
        .await
        .expect("the retarget applies");

    // Then the block belongs to `Roster`, the file imports it, `Host` itself is untouched, and it compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("impl Roster {") && !host.contains("impl Host {"),
        "the block was not retargeted:\n{host}"
    );
    assert!(
        host.contains("use crate::roster::Roster;"),
        "the file does not import the new type:\n{host}"
    );
    assert!(
        host.contains(A_HOST_STRUCT),
        "the struct `Host` was edited:\n{host}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_every_comment_attribute_and_doc_line_of_the_moved_members() {
    // Given an `impl Host` whose member carries a banner, a doc line, an attribute and an inner comment
    let workspace = a_crate_whose_host_reads(&a_host_impl_of(COMMENTED_MEMBERS));

    // When the block is retargeted
    retargeting(&workspace, &["<Host>"], "app::roster::Roster")
        .await
        .expect("the retarget applies");

    // Then everything after the header is the original bytes
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains(&format!("impl Roster {{\n{COMMENTED_MEMBERS}}}\n")),
        "the members were not carried byte for byte:\n{host}"
    );
}

const FOUR_MEMBERS: &str = concat!(
    "    pub fn get(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    /// Replace the count.\n",
    "    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n",
    "\n",
    "    pub fn last(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    pub fn size(&self) -> u32 {\n        self.get() + 1\n    }\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn splits_the_block_at_the_anchored_members_when_they_are_a_proper_subset() {
    // Given an `impl Host` of four members, the last of which calls the first
    let workspace = a_crate_whose_host_reads(&a_host_impl_of(FOUR_MEMBERS));

    // When the middle two are retargeted to `Roster`
    retargeting(
        &workspace,
        &["Host::put", "Host::last"],
        "app::roster::Roster",
    )
    .await
    .expect("the retarget applies");

    // Then there are three blocks in place, the two `Host` headers are the original, and it compiles
    let host = workspace.read("src/host.rs");
    assert_eq!(
        the_impl_blocks_of(&host),
        blocks(&[("Host", "get"), ("Roster", "put last"), ("Host", "size")])
    );
    assert_eq!(
        host.matches("impl Host {\n").count(),
        2,
        "the original header, twice:\n{host}"
    );
    assert_compiles(&workspace);
}

const A_HOST_OF_FOUR_READERS: &str = concat!(
    "    pub fn a(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    pub fn b(&self) -> u32 {\n        self.n + 1\n    }\n",
    "\n",
    "    pub fn c(&self) -> u32 {\n        self.n + 2\n    }\n",
    "\n",
    "    pub fn d(&self) -> u32 {\n        self.n + 3\n    }\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_run_at_the_start_or_the_end_of_the_block_leaves_no_empty_block() {
    // Given two crates of the same four-member `impl Host`
    let at_the_start = a_crate_whose_host_reads(&a_host_impl_of(A_HOST_OF_FOUR_READERS));
    let at_the_end = a_crate_whose_host_reads(&a_host_impl_of(A_HOST_OF_FOUR_READERS));

    // When the first two members are retargeted in one, and the last two in the other
    retargeting(
        &at_the_start,
        &["Host::a", "Host::b"],
        "app::roster::Roster",
    )
    .await
    .expect("the run at the start applies");
    retargeting(&at_the_end, &["Host::c", "Host::d"], "app::roster::Roster")
        .await
        .expect("the run at the end applies");

    // Then each has two blocks, not three, and both compile
    assert_eq!(
        the_impl_blocks_of(&at_the_start.read("src/host.rs")),
        blocks(&[("Roster", "a b"), ("Host", "c d")])
    );
    assert_eq!(
        the_impl_blocks_of(&at_the_end.read("src/host.rs")),
        blocks(&[("Host", "a b"), ("Roster", "c d")])
    );
    assert_compiles(&at_the_start);
    assert_compiles(&at_the_end);
}

const MEMBERS_AROUND_TWO_COMMENTS: &str = concat!(
    "    pub fn get(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    // --- the mutators ---\n",
    "\n",
    "    // kept with put\n",
    "    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n",
    "\n",
    "    pub fn size(&self) -> u32 {\n        self.get() + 1\n    }\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_banner_comment_between_members_stays_with_the_side_it_follows() {
    // Given a free-standing banner after `get`, and a comment attached to `put`
    let workspace = a_crate_whose_host_reads(&a_host_impl_of(MEMBERS_AROUND_TWO_COMMENTS));

    // When `put` is retargeted
    retargeting(&workspace, &["Host::put"], "app::roster::Roster")
        .await
        .expect("the retarget applies");

    // Then the banner is in the `Host` block that precedes `Roster`'s, and the attached comment moved with `put`
    let host = workspace.read("src/host.rs");
    let the_new_block = host.find("impl Roster {").expect("the new block");
    assert!(
        host.find("// --- the mutators ---").expect("the banner") < the_new_block,
        "the banner left the `Host` block:\n{host}"
    );
    assert!(
        host.find("// kept with put").expect("the attached comment") > the_new_block,
        "the attached comment did not travel with `put`:\n{host}"
    );
}

const A_WRAPPER_OF_THREE: &str = concat!(
    "pub struct Wrapper<T> {\n    pub(crate) v: T,\n}\n\n",
    "impl<T> Wrapper<T>\nwhere\n    T: Copy,\n{\n",
    "    pub fn first(&self) -> T {\n        self.v\n    }\n",
    "\n",
    "    pub fn second(&self) -> T {\n        self.v\n    }\n",
    "\n",
    "    pub fn third(&self) -> T {\n        self.v\n    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_generic_parameter_list_and_where_clause_of_the_header() {
    // Given an `impl<T> Wrapper<T> where T: Copy`, and a `Pair<T>` to move a member to
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", A_WRAPPER_OF_THREE),
        (
            "src/roster.rs",
            "pub struct Pair<T> {\n    pub(crate) v: T,\n}\n",
        ),
    ]);

    // When the middle member is retargeted to `app::roster::Pair<T>`
    retargeting(&workspace, &["Wrapper::second"], "app::roster::Pair<T>")
        .await
        .expect("the retarget applies");

    // Then the new block and both old ones keep the parameter list and the `where` clause
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("impl<T> Pair<T>\nwhere\n    T: Copy,\n{\n"),
        "the new header lost its parameters or its where clause:\n{host}"
    );
    assert_eq!(
        host.matches("impl<T> Wrapper<T>\nwhere\n    T: Copy,\n{\n")
            .count(),
        2,
        "an old header lost its parameters or its where clause:\n{host}"
    );
    assert_compiles(&workspace);
}

const A_HOST_THAT_BUILDS_ITSELF: &str = concat!(
    "pub struct Host {\n    pub(crate) n: u32,\n}\n\n",
    "impl Host {\n",
    "    pub const LIMIT: u32 = 10;\n",
    "\n",
    "    pub fn build(n: u32) -> Host {\n        Host { n }\n    }\n",
    "\n",
    "    pub fn rebuilt(&self) -> Host {\n",
    "        let _within = Host::LIMIT;\n",
    "        Host::build(self.n)\n",
    "    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn re_points_a_path_that_names_a_moved_associated_function_through_the_old_type() {
    // Given a moved method that calls a moved associated function and reads one that stays
    let workspace = a_crate_whose_host_reads(A_HOST_THAT_BUILDS_ITSELF);

    // When `build` and `rebuilt` are retargeted
    retargeting(
        &workspace,
        &["Host::build", "Host::rebuilt"],
        "app::roster::Roster",
    )
    .await
    .expect("the retarget applies");

    // Then the moved call names the new type, the constant that stayed is still read through `Host`, and it compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("Roster::build(self.n)") && !host.contains("Host::build"),
        "the path to a moved function still names the old type:\n{host}"
    );
    assert!(
        host.contains("Host::LIMIT"),
        "a path to a member that stayed was re-pointed:\n{host}"
    );
    assert_compiles(&workspace);
}

const A_HOST_WHOSE_METHOD_READS_A_FIELD_ROSTER_LACKS: &str = concat!(
    "pub struct Host {\n    pub(crate) n: u32,\n    pub(crate) count: u32,\n}\n\n",
    "impl Host {\n",
    "    pub fn get(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    pub fn bump(&mut self) {\n        self.count += 1;\n    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn names_a_field_the_new_type_lacks_before_anything_is_written() {
    // Given a method that reads `self.count`, which `Roster` does not declare
    let workspace = a_crate_whose_host_reads(A_HOST_WHOSE_METHOD_READS_A_FIELD_ROSTER_LACKS);
    let before = workspace.read("src/host.rs");
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::bump"]).await;
    let plan = workspace.a_plan_of(&[a_retarget_op(&anchor, "app::roster::Roster")]);

    // When the plan is checked deeply, and then applied
    let findings = checking_the_plan(&workspace, plan, true)
        .await
        .expect("a deep check runs");
    let refusal = applying_a_plan_of(&workspace, &[a_retarget_op(&anchor, "app::roster::Roster")])
        .await
        .expect_err("the apply refuses");

    // Then the check names the member, the field and what `Roster` has; the apply refuses; nothing was written
    assert!(
        findings.len() == 1
            && findings[0].contains("`bump`")
            && findings[0].contains("`self.count`")
            && findings[0].contains("(its fields: n)"),
        "the deep check did not name the missing field: {findings:?}"
    );
    assert!(
        refusal.contains("this seam cannot be cut here:"),
        "the apply did not refuse as a seam: {refusal}"
    );
    assert_eq!(
        workspace.read("src/host.rs"),
        before,
        "the refusal left the file edited"
    );
}

const TWO_BLOCKS_OF_HOST: &str = concat!(
    "pub struct Host {\n    pub(crate) n: u32,\n}\n\n",
    "impl Host {\n    pub fn get(&self) -> u32 {\n        self.n\n    }\n}\n",
    "\n",
    "impl Host {\n    pub fn size(&self) -> u32 {\n        self.n + 1\n    }\n}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_anchor_whose_members_sit_in_two_impl_blocks() {
    // Given two `impl Host` blocks of one member each, side by side
    let workspace = a_crate_whose_host_reads(TWO_BLOCKS_OF_HOST);
    let before = workspace.read("src/host.rs");

    // When one operation anchors on both blocks
    let refusal = retargeting(&workspace, &["<Host>#1", "<Host>#2"], "app::roster::Roster")
        .await
        .expect_err("the retarget refuses");

    // Then it is refused as a seam, naming the blocks, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:")
            && refusal.contains("sit in more than one `impl` block"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_retarget_an_impl_to_the_type_it_already_is() {
    // Given an `impl Host`
    let workspace = a_crate_whose_host_reads(&a_host_impl_of(COMMENTED_MEMBERS));
    let before = workspace.read("src/host.rs");

    // When it is retargeted to `Host`
    let refusal = retargeting(&workspace, &["<Host>"], "app::host::Host")
        .await
        .expect_err("the retarget refuses");

    // Then it is refused as a seam, saying there is nothing to retarget, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:")
            && refusal.contains("`Host` is already the self type of this `impl`"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_use_that_would_clash_with_a_type_the_file_already_binds() {
    // Given a `host` module that already binds `Roster` to a different type
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod host;\npub mod other;\npub mod roster;\n",
        ),
        (
            "src/host.rs",
            &format!(
                "use crate::other::Roster;\n\n{}",
                a_host_impl_of(COMMENTED_MEMBERS)
            ),
        ),
        ("src/other.rs", "pub struct Roster;\n"),
        ("src/roster.rs", A_ROSTER),
    ]);
    let before = workspace.read("src/host.rs");

    // When the block is retargeted to `app::roster::Roster`
    let refusal = retargeting(&workspace, &["<Host>"], "app::roster::Roster")
        .await
        .expect_err("the retarget refuses");

    // Then it is refused as a seam, naming the clash, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:")
            && refusal.contains("`Roster` is already bound in `src/host.rs`"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_left_pointing_at_the_old_type_fails_the_compile_gate_and_is_left_alone() {
    // Given a caller in another file that calls `h.get()` and is not in the plan
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod caller;\npub mod host;\npub mod roster;\n",
        ),
        ("src/host.rs", &a_host_impl_of(COMMENTED_MEMBERS)),
        ("src/roster.rs", A_ROSTER),
        (
            "src/caller.rs",
            "use crate::host::Host;\n\npub fn read(h: &Host) -> u32 {\n    h.get()\n}\n",
        ),
    ]);
    let caller_before = workspace.read("src/caller.rs");

    // When the whole block is retargeted
    let failure = retargeting(&workspace, &["<Host>"], "app::roster::Roster")
        .await
        .expect_err("the compile gate fails the run");

    // Then the gate names the missing method, and the caller was not edited
    assert!(
        failure.contains("no longer compiles") && failure.contains("E0599"),
        "unexpected failure: {failure}"
    );
    assert_eq!(
        workspace.read("src/caller.rs"),
        caller_before,
        "the caller was edited"
    );
}

/// The `svc` module of the crates below: its three children, each a file.
const SVC_CHILDREN: &str = "pub mod host;\npub mod other;\npub mod roster;\n";

/// A crate whose type `Host` lives in `app::svc::host`, which reads `imports` above it; its siblings
/// `roster` and `other` each declare a `Roster`, only `roster`'s with the field `n`.
fn a_crate_whose_svc_host_reads(imports: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod svc;\n"),
        ("src/svc.rs", SVC_CHILDREN),
        (
            "src/svc/host.rs",
            &format!("{imports}\n{}", a_host_impl_of(COMMENTED_MEMBERS)),
        ),
        ("src/svc/other.rs", "pub struct Roster;\n"),
        ("src/svc/roster.rs", A_ROSTER),
    ])
}

/// A plan retargeting the whole `impl Host` of `file`, whose module path is `module`, to `to_type`.
async fn retargeting_the_block_in(
    workspace: &AFixtureWorkspace,
    file: &str,
    module: &str,
    to_type: &str,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let block = format!("{module}::<Host>");
    let anchor = the_anchor_over(workspace, file, &[block.as_str()]).await;
    applying_a_plan_of(workspace, &[a_retarget_op(&anchor, to_type)]).await
}

#[tokio::test(flavor = "multi_thread")]
async fn retargets_a_block_in_a_file_that_imports_the_new_type_by_a_super_path() {
    // Given a `host` that already imports `Roster` through its parent
    let workspace = a_crate_whose_svc_host_reads("use super::roster::Roster;\n");

    // When its `impl Host` is retargeted to `app::svc::roster::Roster`
    retargeting_the_block_in(
        &workspace,
        "src/svc/host.rs",
        "app::svc::host",
        "app::svc::roster::Roster",
    )
    .await
    .expect("the retarget applies");

    // Then the block is `Roster`'s, the file keeps its one import of it, and the tree compiles
    let host = workspace.read("src/svc/host.rs");
    assert_eq!(
        the_impl_blocks_of(&host),
        blocks(&[("Roster", "get")]),
        "the block was not retargeted:\n{host}"
    );
    assert_eq!(
        host.matches("Roster;").count(),
        1,
        "the file does not bind `Roster` exactly once:\n{host}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn retargets_a_block_in_a_file_that_imports_the_new_type_from_a_child_module_by_a_relative_path(
) {
    // Given a `svc` that declares `roster` and imports its `Roster` by the 2018 child-relative path
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod svc;\n"),
        (
            "src/svc.rs",
            &format!(
                "pub mod roster;\n\nuse roster::Roster;\n\n{}",
                a_host_impl_of(COMMENTED_MEMBERS)
            ),
        ),
        ("src/svc/roster.rs", A_ROSTER),
    ]);

    // When its `impl Host` is retargeted to `app::svc::roster::Roster`
    retargeting_the_block_in(
        &workspace,
        "src/svc.rs",
        "app::svc",
        "app::svc::roster::Roster",
    )
    .await
    .expect("the retarget applies");

    // Then the block is `Roster`'s, no second import is written, and the tree compiles
    let svc = workspace.read("src/svc.rs");
    assert_eq!(
        the_impl_blocks_of(&svc),
        blocks(&[("Roster", "get")]),
        "the block was not retargeted:\n{svc}"
    );
    assert_eq!(
        svc.matches("Roster;").count(),
        1,
        "the file does not bind `Roster` exactly once:\n{svc}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn still_refuses_a_super_import_of_a_different_type_of_the_same_name() {
    // Given a `host` that imports the *other* `Roster` through its parent
    let workspace = a_crate_whose_svc_host_reads("use super::other::Roster;\n");
    let before = workspace.read("src/svc/host.rs");

    // When its `impl Host` is retargeted to `app::svc::roster::Roster`
    let refusal = retargeting_the_block_in(
        &workspace,
        "src/svc/host.rs",
        "app::svc::host",
        "app::svc::roster::Roster",
    )
    .await
    .expect_err("the retarget refuses");

    // Then it is refused as a clash, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:")
            && refusal.contains("`Roster` is already bound in `src/svc/host.rs`"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/svc/host.rs"), before);
}
