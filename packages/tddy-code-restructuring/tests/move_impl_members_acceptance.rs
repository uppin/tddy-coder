//! `move_impl_members` moves a run of members of one inherent `impl` into an `impl` of the same type
//! in another module of the same crate, against a live rust-analyzer.
//!
//! No operation could do this: `move_item` refuses a range inside an `impl`, and `extract_module`
//! only writes a new child module of the file and widens every private member to `pub(crate)`.
//! `cargo check` is the assertion no edit that merely looks right can satisfy — a member that lost
//! its type, a call that no longer sees a private method, or a field read from outside the struct's
//! module all fail it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    applying_a_plan_of, applying_keeping_the_account, assert_compiles,
    assert_compiles_with_its_tests, assert_lints_clean, the_widenings_in, AFixtureWorkspace,
};
use same_crate::{an_app_holding, blocks, the_anchor_over, the_impl_blocks_of};
use tddy_code_restructuring::{Anchor, RefactorOp};

/// `Host` with a private `bump` that `tick` (left behind) calls, and a sibling module that calls it
/// too; `host::counting` exists and holds one free function.
const A_HOST_WHOSE_BUMP_IS_CALLED_FROM_HOST_AND_A_SIBLING: &str = concat!(
    "mod counting;\n",
    "mod sibling;\n",
    "\n",
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
    "    pub fn tick(&mut self) {\n",
    "        self.bump();\n",
    "        sibling::poke(self);\n",
    "    }\n",
    "}\n",
);
const A_SIBLING_THAT_BUMPS: &str =
    "pub(super) fn poke(host: &mut super::Host) {\n    host.bump();\n}\n";
const A_COUNTING_MODULE: &str = "pub fn noop() {}\n";

/// `Host` whose public `twice` reads the private field `n`, calls the private `bump` and the
/// module's private `step`.
const A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS: &str = concat!(
    "mod inner;\n",
    "\n",
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "fn step() -> u32 {\n",
    "    1\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn new() -> Host {\n",
    "        Host { n: 0 }\n",
    "    }\n",
    "\n",
    "    fn bump(&mut self) {\n",
    "        self.n += step();\n",
    "    }\n",
    "\n",
    "    pub fn twice(&mut self) {\n",
    "        self.bump();\n",
    "        self.n += step();\n",
    "    }\n",
    "}\n",
);
const AN_INNER_MODULE: &str = "pub fn inner() {}\n";

/// `Host` with a private associated function its file's tests call through the type.
const A_HOST_WHOSE_TESTS_CALL_MAKE: &str = concat!(
    "mod counting;\n",
    "\n",
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn new() -> Host {\n",
    "        Host::make()\n",
    "    }\n",
    "\n",
    "    fn make() -> Host {\n",
    "        Host { n: 0 }\n",
    "    }\n",
    "}\n",
    "\n",
    "#[cfg(test)]\n",
    "mod tests {\n",
    "    use super::*;\n",
    "\n",
    "    #[test]\n",
    "    fn makes_an_empty_host() {\n",
    "        assert_eq!(Host::make().n, 0);\n",
    "    }\n",
    "}\n",
);

/// Two `impl Host` blocks of one member each, side by side.
const TWO_BLOCKS_OF_HOST: &str = concat!(
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn a(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn b(&self) -> u32 {\n",
    "        self.n + 1\n",
    "    }\n",
    "}\n",
);

/// `read` exists only in `impl Meter for Host`.
const A_HOST_THAT_IMPLEMENTS_A_TRAIT: &str = concat!(
    "pub trait Meter {\n",
    "    fn read(&self) -> u32;\n",
    "}\n",
    "\n",
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Meter for Host {\n",
    "    fn read(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
);

/// A macro invocation stands between `a` and `b`.
const A_HOST_WITH_A_MACRO_BETWEEN_MEMBERS: &str = concat!(
    "macro_rules! nothing {\n",
    "    () => {};\n",
    "}\n",
    "\n",
    "pub struct Host {\n",
    "    n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn a(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    nothing!();\n",
    "\n",
    "    pub fn b(&self) -> u32 {\n",
    "        self.n + 1\n",
    "    }\n",
    "}\n",
);

const LIB_OF_HOST_AND_ROSTER: &str = "pub mod host;\npub mod roster;\n";
const AN_EMPTY_ROSTER: &str = "pub fn roster() {}\n";
const A_ROSTER_WITH_ITS_OWN_STEP: &str =
    "pub fn roster() -> u32 {\n    step()\n}\n\nfn step() -> u32 {\n    2\n}\n";
const A_ROSTER_WITH_A_HOST_BLOCK: &str =
    "use crate::host::Host;\n\nimpl Host {\n    pub fn other(&self) {}\n}\n";

/// A `move_impl_members` of the members `anchor` names into the module `to`.
fn a_member_move_op(anchor: &Anchor, to: &str) -> RefactorOp {
    let op = serde_json::json!({ "op": "move_impl_members", "anchor": anchor, "to": to });
    serde_json::from_value(op).expect("a `move_impl_members` operation parses")
}

/// A `move_impl_members` that creates the module `name` in `parent` first.
fn a_member_move_into_a_new_module_op(anchor: &Anchor, parent: &str, name: &str) -> RefactorOp {
    let op = serde_json::json!({
        "op": "move_impl_members", "anchor": anchor, "to": parent, "name": name,
    });
    serde_json::from_value(op).expect("a `move_impl_members` operation parses")
}

/// The anchor `restructure anchors` emits over `members` of `src/host.rs`, each written as the
/// `host` module's path to it (`Host::bump`).
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

/// One `items` anchor naming `first` and `second`, each anchored on its own first: the shape a plan
/// author could write by hand over two members the anchors command would not put in one run.
async fn one_anchor_over_two_separately_anchored_members(
    workspace: &AFixtureWorkspace,
    first: &str,
    second: &str,
) -> Anchor {
    let one = serde_json::to_value(the_anchor_over_members_of_host(workspace, &[first]).await)
        .expect("an anchor serialises");
    let other = serde_json::to_value(the_anchor_over_members_of_host(workspace, &[second]).await)
        .expect("an anchor serialises");
    let mut joined = one.clone();
    for field in ["items", "fingerprints"] {
        let mut values = one[field].as_array().expect("a list").clone();
        values.extend(other[field].as_array().expect("a list").iter().cloned());
        joined[field] = serde_json::Value::Array(values);
    }
    serde_json::from_value(joined).expect("the joined anchor reads back")
}

fn an_app_whose_host_reads(host: &str, others: &[(&str, &str)]) -> AFixtureWorkspace {
    let mut files = vec![
        ("src/lib.rs", LIB_OF_HOST_AND_ROSTER),
        ("src/host.rs", host),
    ];
    files.extend_from_slice(others);
    an_app_holding(&files)
}

/// The widenings an apply of `op` reported, after it applied.
async fn the_widenings_of_applying(workspace: &AFixtureWorkspace, op: RefactorOp) -> Vec<String> {
    let (applied, account) = applying_keeping_the_account(workspace, &[op], false).await;
    applied.expect("the member move applies");
    the_widenings_in(&account)
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_members_of_two_blocks() {
    // Given two `impl Host` blocks of one member each
    let workspace =
        an_app_whose_host_reads(TWO_BLOCKS_OF_HOST, &[("src/roster.rs", AN_EMPTY_ROSTER)]);
    let before = workspace.read("src/host.rs");
    let anchor =
        one_anchor_over_two_separately_anchored_members(&workspace, "Host::a", "Host::b").await;

    // When one operation moves a member of each
    let refusal = applying_a_plan_of(&workspace, &[a_member_move_op(&anchor, "app::roster")])
        .await
        .expect_err("the member move refuses");

    // Then it is refused as a seam, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
    assert_eq!(workspace.read("src/roster.rs"), AN_EMPTY_ROSTER);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_member_of_a_trait_impl() {
    // Given `read`, defined only in `impl Meter for Host`
    let workspace = an_app_whose_host_reads(
        A_HOST_THAT_IMPLEMENTS_A_TRAIT,
        &[("src/roster.rs", AN_EMPTY_ROSTER)],
    );
    let before = workspace.read("src/host.rs");
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::read"]).await;

    // When it is moved alone
    let refusal = applying_a_plan_of(&workspace, &[a_member_move_op(&anchor, "app::roster")])
        .await
        .expect_err("the member move refuses");

    // Then the refusal says a trait `impl` moves whole, and nothing was written
    assert!(
        refusal.contains("a trait `impl` moves whole with `move_item`"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_macro_invocation_inside_the_run() {
    // Given a macro invocation between the members `a` and `b`
    let workspace = an_app_whose_host_reads(
        A_HOST_WITH_A_MACRO_BETWEEN_MEMBERS,
        &[("src/roster.rs", AN_EMPTY_ROSTER)],
    );
    let before = workspace.read("src/host.rs");
    let anchor =
        one_anchor_over_two_separately_anchored_members(&workspace, "Host::a", "Host::b").await;

    // When both members are moved in one operation
    let refusal = applying_a_plan_of(&workspace, &[a_member_move_op(&anchor, "app::roster")])
        .await
        .expect_err("the member move refuses");

    // Then it is refused as a seam, and the invocation stays where it was
    assert!(
        refusal.contains("this seam cannot be cut here:"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn moves_private_methods_a_member_left_behind_calls_into_an_existing_module() {
    // Given a private `bump` that `tick` and a sibling module call, and an existing `host::counting`
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_BUMP_IS_CALLED_FROM_HOST_AND_A_SIBLING,
        &[
            ("src/roster.rs", AN_EMPTY_ROSTER),
            ("src/host/counting.rs", A_COUNTING_MODULE),
            ("src/host/sibling.rs", A_SIBLING_THAT_BUMPS),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::bump"]).await;

    // When `bump` moves into `host::counting`
    let widenings =
        the_widenings_of_applying(&workspace, a_member_move_op(&anchor, "app::host::counting"))
            .await;

    // Then a new `impl Host` there holds it, widened only as far as `host` and its children need,
    // and the widening is reported
    let counting = workspace.read("src/host/counting.rs");
    assert_eq!(
        the_impl_blocks_of(&counting),
        blocks(&[("Host", "bump")]),
        "`bump` did not land in a block of `Host`:\n{counting}"
    );
    assert!(
        counting.contains("    pub(super) fn bump(&mut self) {"),
        "`bump` is not `pub(super)`:\n{counting}"
    );
    assert!(
        widenings
            .iter()
            .any(|line| line.contains("`Host::bump` private -> pub(super)")),
        "the widening of `bump` was not reported: {widenings:?}"
    );
    assert_eq!(
        the_impl_blocks_of(&workspace.read("src/host.rs")),
        blocks(&[("Host", "new tick")])
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_the_field_and_stayed_method_for_a_destination_outside_the_origin() {
    // Given `twice`, which reads the private field `n`, calls the private `bump` and the private `step`
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS,
        &[
            ("src/roster.rs", AN_EMPTY_ROSTER),
            ("src/host/inner.rs", AN_INNER_MODULE),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::twice"]).await;

    // When `twice` moves to `roster`, outside `host`
    let widenings =
        the_widenings_of_applying(&workspace, a_member_move_op(&anchor, "app::roster")).await;

    // Then the field, the method and the function it reaches are widened, and each is reported
    for reached in ["`Host::n`", "`Host::bump`", "`step`"] {
        assert!(
            widenings.iter().any(|line| line.contains(reached)),
            "{reached} was not reported widened: {widenings:?}"
        );
    }
    assert_eq!(
        the_impl_blocks_of(&workspace.read("src/roster.rs")),
        blocks(&[("Host", "twice")])
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn widens_nothing_but_the_moved_members_for_a_child_destination() {
    // Given the same `twice`, and the child module `host::inner`
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS,
        &[
            ("src/roster.rs", AN_EMPTY_ROSTER),
            ("src/host/inner.rs", AN_INNER_MODULE),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::twice"]).await;

    // When `twice` (already `pub`) moves into the child
    let widenings =
        the_widenings_of_applying(&workspace, a_member_move_op(&anchor, "app::host::inner")).await;

    // Then a child sees its parent's private items: nothing is widened, and it still compiles
    assert_eq!(widenings, Vec::<String>::new());
    assert_eq!(
        the_impl_blocks_of(&workspace.read("src/host/inner.rs")),
        blocks(&[("Host", "twice")])
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_the_destination_named_on_the_line() {
    // Given `bump`, and no `host::counting` yet
    let host = A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS;
    let workspace = an_app_whose_host_reads(
        host,
        &[
            ("src/roster.rs", AN_EMPTY_ROSTER),
            ("src/host/inner.rs", AN_INNER_MODULE),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::bump"]).await;

    // When `bump` moves into a module `counting` the line creates in `host`
    applying_a_plan_of(
        &workspace,
        &[a_member_move_into_a_new_module_op(
            &anchor,
            "app::host",
            "counting",
        )],
    )
    .await
    .expect("the member move applies");

    // Then `host` declares the module, which holds `bump` in a block of `Host`
    assert!(
        workspace.read("src/host.rs").contains("mod counting;"),
        "`host` does not declare `counting`:\n{}",
        workspace.read("src/host.rs")
    );
    assert_eq!(
        the_impl_blocks_of(&workspace.read("src/host/counting.rs")),
        blocks(&[("Host", "bump")])
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_helper_the_destination_binds_to_another_item() {
    // Given `twice`, which calls `host`'s `step`, and a `roster` that declares a `step` of its own
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS,
        &[
            ("src/roster.rs", A_ROSTER_WITH_ITS_OWN_STEP),
            ("src/host/inner.rs", AN_INNER_MODULE),
        ],
    );
    let before = workspace.read("src/host.rs");
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::twice"]).await;

    // When `twice` moves into `roster`
    let refusal = applying_a_plan_of(&workspace, &[a_member_move_op(&anchor, "app::roster")])
        .await
        .expect_err("the member move refuses");

    // Then the refusal names the name bound twice, and nothing was written
    assert!(
        refusal.contains("this seam cannot be cut here:") && refusal.contains("`step`"),
        "unexpected refusal: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
    assert_eq!(workspace.read("src/roster.rs"), A_ROSTER_WITH_ITS_OWN_STEP);
}

#[tokio::test(flavor = "multi_thread")]
async fn appends_to_the_destination_block_of_the_same_type() {
    // Given a `roster` holding one `impl Host`
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_TWICE_READS_PRIVATE_THINGS,
        &[
            ("src/roster.rs", A_ROSTER_WITH_A_HOST_BLOCK),
            ("src/host/inner.rs", AN_INNER_MODULE),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::twice"]).await;

    // When `twice` moves into `roster`
    applying_a_plan_of(&workspace, &[a_member_move_op(&anchor, "app::roster")])
        .await
        .expect("the member move applies");

    // Then it joins that block, and no second block opens
    assert_eq!(
        the_impl_blocks_of(&workspace.read("src/roster.rs")),
        blocks(&[("Host", "other twice")])
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_inline_tests_still_call_a_moved_associated_function_through_the_type() {
    // Given a private `make` that `new` and the file's tests call as `Host::make()`
    let workspace = an_app_whose_host_reads(
        A_HOST_WHOSE_TESTS_CALL_MAKE,
        &[
            ("src/roster.rs", AN_EMPTY_ROSTER),
            ("src/host/counting.rs", A_COUNTING_MODULE),
        ],
    );
    let anchor = the_anchor_over_members_of_host(&workspace, &["Host::make"]).await;

    // When `make` moves into `host::counting`
    applying_a_plan_of(
        &workspace,
        &[a_member_move_op(&anchor, "app::host::counting")],
    )
    .await
    .expect("the member move applies");

    // Then the tests' call is unchanged, and the tests compile against the moved function
    assert!(
        workspace
            .read("src/host.rs")
            .contains("        assert_eq!(Host::make().n, 0);"),
        "the tests' call changed:\n{}",
        workspace.read("src/host.rs")
    );
    assert_compiles_with_its_tests(&workspace);
}
