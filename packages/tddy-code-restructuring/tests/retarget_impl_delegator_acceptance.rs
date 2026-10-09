//! `retarget_impl` with `variant: "leave_delegator"` leaves a forwarding method on the old type for
//! every moved method, against a live rust-analyzer.
//!
//! After a method moves to another type its callers break (`E0599`). `#carve` 17 hand-wrote seven
//! forwarding delegators to keep them compiling, and only the compiler's `dead_code` warning found the
//! wrappers nobody called. The delegator keeps each moved method's slot in the old block, with the
//! member's own signature and outer attributes and a body that forwards through `expr`; the moved
//! members follow in an `impl <New>` block, and a delegator nothing calls is noted.
//!
//! `cargo check` is the assertion: a caller left byte-identical compiles only if the delegator is
//! there and forwards correctly.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    applying_a_plan_of, applying_keeping_the_account, assert_compiles_with_its_tests,
    checking_deep_keeping_the_account, AFixtureWorkspace,
};
use same_crate::{an_app_holding, blocks, the_anchor_over, the_impl_blocks_of};
use tddy_code_restructuring::RefactorOp;

const LIB: &str = "pub mod caller;\npub mod host;\npub mod roster;\n";
const A_ROSTER: &str = "pub struct Roster {\n    pub(crate) n: u32,\n}\n";

/// A host that holds its `Roster`, so `self.roster` reaches the new type.
const A_HOST_STRUCT: &str = concat!(
    "use crate::roster::Roster;\n",
    "\n",
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "    pub(crate) roster: Roster,\n",
    "}\n",
    "\n",
);

/// Three methods: `put` carries a doc comment and an attribute, and `size` calls `get`.
const THREE_METHODS: &str = concat!(
    "impl Host {\n",
    "    pub fn get(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    /// Replace the count.\n",
    "    #[inline]\n",
    "    pub fn put(&mut self, v: u32) {\n",
    "        self.n = v;\n",
    "    }\n",
    "\n",
    "    pub fn size(&self) -> u32 {\n",
    "        self.get() + 1\n",
    "    }\n",
    "}\n",
);

/// A caller in another file of `get` and `put`; nothing calls `size`.
const A_CALLER: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "pub fn run(h: &mut Host) -> u32 {\n",
    "    h.put(1);\n",
    "    h.get()\n",
    "}\n",
);

/// The three-method host, with [`A_CALLER`] calling two of its methods from another file.
fn a_crate_whose_host_has_a_caller() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", &format!("{A_HOST_STRUCT}{THREE_METHODS}")),
        ("src/roster.rs", A_ROSTER),
        ("src/caller.rs", A_CALLER),
    ])
}

/// A host holding `members`, with no caller.
fn a_crate_whose_host_holds(members: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", "pub mod host;\npub mod roster;\n"),
        ("src/host.rs", &format!("{A_HOST_STRUCT}{members}")),
        ("src/roster.rs", A_ROSTER),
    ])
}

/// A retarget of `members` of `src/host.rs` (`<Host>` for the whole block) to `Roster`, leaving
/// delegators that forward through `self.roster`.
async fn a_retarget_leaving_delegators(
    workspace: &AFixtureWorkspace,
    members: &[&str],
) -> RefactorOp {
    let paths: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let anchor = the_anchor_over(workspace, "src/host.rs", &paths).await;
    serde_json::from_value(serde_json::json!({
        "op": "retarget_impl",
        "anchor": anchor,
        "to_type": "app::roster::Roster",
        "variant": "leave_delegator",
        "expr": "self.roster",
    }))
    .expect("a `retarget_impl` operation with a delegator deserializes")
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_forwarding_method_on_the_old_type_so_callers_keep_compiling() {
    // Given a host of three methods and a caller of two of them in another file
    let workspace = a_crate_whose_host_has_a_caller();
    let retarget = a_retarget_leaving_delegators(&workspace, &["<Host>"]).await;

    // When the whole block is retargeted to `Roster`, leaving delegators
    applying_a_plan_of(&workspace, &[retarget])
        .await
        .expect("the retarget applies");

    // Then `Host` keeps a forwarding method per moved method, the caller is untouched, and it compiles
    let host = workspace.read("src/host.rs");
    assert_eq!(
        the_impl_blocks_of(&host),
        blocks(&[("Host", "get put size"), ("Roster", "get put size")])
    );
    assert!(
        host.contains("    pub fn get(&self) -> u32 {\n        self.roster.get()\n    }\n"),
        "`get` does not forward:\n{host}"
    );
    assert_eq!(workspace.read("src/caller.rs"), A_CALLER);
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proper_subset_leaves_its_delegators_in_place_and_the_new_block_follows_the_old() {
    // Given a host of three methods
    let workspace = a_crate_whose_host_has_a_caller();
    let retarget = a_retarget_leaving_delegators(&workspace, &["Host::put"]).await;

    // When only `put` is retargeted, leaving a delegator
    applying_a_plan_of(&workspace, &[retarget])
        .await
        .expect("the retarget applies");

    // Then the old block keeps its shape with `put` forwarding in place, `impl Roster` follows, and it compiles
    let host = workspace.read("src/host.rs");
    assert_eq!(
        the_impl_blocks_of(&host),
        blocks(&[("Host", "get put size"), ("Roster", "put")])
    );
    assert!(
        host.contains("    }\n}\n\nimpl Roster {\n"),
        "`impl Roster` does not follow the old block after one blank line:\n{host}"
    );
    assert_eq!(workspace.read("src/caller.rs"), A_CALLER);
    assert_compiles_with_its_tests(&workspace);
}

/// An `async` method and an associated function with no receiver.
const AN_ASYNC_METHOD_AND_AN_ASSOCIATED_FUNCTION: &str = concat!(
    "impl Host {\n",
    "    pub async fn load(&self, id: u32) -> u32 {\n",
    "        id + self.n\n",
    "    }\n",
    "\n",
    "    pub fn doubled(n: u32) -> u32 {\n",
    "        n * 2\n",
    "    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn forwards_an_async_method_with_await_and_an_associated_function_through_the_new_type() {
    // Given an `async` method and an associated function
    let workspace = a_crate_whose_host_holds(AN_ASYNC_METHOD_AND_AN_ASSOCIATED_FUNCTION);
    let retarget = a_retarget_leaving_delegators(&workspace, &["<Host>"]).await;

    // When the block is retargeted to `Roster`, leaving delegators
    applying_a_plan_of(&workspace, &[retarget])
        .await
        .expect("the retarget applies");

    // Then the method's delegator awaits, the function's forwards through `Roster`, and it compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("    pub async fn load(&self, id: u32) -> u32 {\n        self.roster.load(id).await\n    }\n")
            && host.contains("    pub fn doubled(n: u32) -> u32 {\n        Roster::doubled(n)\n    }\n"),
        "the delegators do not forward as their members are called:\n{host}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delegator_keeps_the_signature_and_outer_attributes_and_the_doc_comment_stays_on_the_moved_member(
) {
    // Given `put`, carrying a doc comment and `#[inline]`
    let workspace = a_crate_whose_host_has_a_caller();
    let retarget = a_retarget_leaving_delegators(&workspace, &["Host::put"]).await;

    // When it is retargeted, leaving a delegator
    applying_a_plan_of(&workspace, &[retarget])
        .await
        .expect("the retarget applies");

    // Then the delegator has the attribute and the signature but not the doc, which moved with `put`
    let host = workspace.read("src/host.rs");
    let roster_block = &host[host.find("impl Roster {").expect("the new block")..];
    assert!(
        host.contains("    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.roster.put(v)\n    }\n"),
        "the delegator lost its attribute or signature:\n{host}"
    );
    assert_eq!(host.matches("/// Replace the count.").count(), 1);
    assert!(
        roster_block.contains("    /// Replace the count.\n    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n"),
        "the moved member lost its doc comment:\n{host}"
    );
}

/// Three members a forwarding method cannot be written for.
const THREE_UNFORWARDABLE_MEMBERS: &str = concat!(
    "impl Host {\n",
    "    pub fn pair(&self, (a, b): (u32, u32)) -> u32 {\n",
    "        a + b\n",
    "    }\n",
    "\n",
    "    pub const LIMIT: u32 = 3;\n",
    "\n",
    "    pub const fn zero(&self) -> u32 {\n",
    "        0\n",
    "    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_delegator_for_a_pattern_parameter_an_associated_const_and_a_const_fn_naming_each(
) {
    // Given a block whose every member cannot be forwarded
    let workspace = a_crate_whose_host_holds(THREE_UNFORWARDABLE_MEMBERS);
    let before = workspace.read("src/host.rs");
    let retarget = a_retarget_leaving_delegators(&workspace, &["<Host>"]).await;

    // When it is retargeted, leaving delegators
    let refusal = applying_a_plan_of(&workspace, &[retarget])
        .await
        .expect_err("members that cannot forward are refused");

    // Then one refusal names all three, and the file is untouched
    assert!(
        refusal
            .contains("`pair` takes `(a, b)` as a pattern, so a forwarding method cannot name it")
            && refusal
                .contains("`LIMIT` is an associated const or type: it has no body to forward")
            && refusal.contains("`zero` is a `const fn`, which cannot forward to a non-const call"),
        "the refusal does not name every member: {refusal}"
    );
    assert_eq!(workspace.read("src/host.rs"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delegator_with_no_caller_left_is_noted_by_the_deep_check_and_the_apply() {
    // Given a host whose `size` nothing outside the block calls
    let workspace = a_crate_whose_host_has_a_caller();
    let retarget = a_retarget_leaving_delegators(&workspace, &["<Host>"]).await;

    // When the whole block is checked deep and then applied, leaving delegators
    let (found, deep_account) =
        checking_deep_keeping_the_account(&workspace, std::slice::from_ref(&retarget)).await;
    let (applied, apply_account) =
        applying_keeping_the_account(&workspace, &[retarget], false).await;

    // Then both note `size`'s delegator, and no other
    let note = "retarget_impl: the delegator `Host::size` has no caller in the workspace: remove it, or retarget without leave_delegator";
    assert_eq!(found.expect("the deep check runs"), Vec::<String>::new());
    applied.expect("the retarget applies");
    assert_eq!(the_delegator_notes_in(&deep_account), [note]);
    assert_eq!(the_delegator_notes_in(&apply_account), [note]);
}

/// The delegator notes of an account, each from its `retarget_impl:` on.
fn the_delegator_notes_in(account: &[String]) -> Vec<&str> {
    const MARK: &str = "retarget_impl: the delegator";
    account
        .iter()
        .filter_map(|line| line.find(MARK).map(|at| &line[at..]))
        .collect()
}
