//! `retarget_impl` with `variant: "leave_delegator"` leaves a forwarding method on the old type for
//! every member that moved, so callers of the old type keep compiling, against a live
//! rust-analyzer.
//!
//! The forwarding method is the signature of the moved member (visibility to the body's `{`,
//! verbatim) over a body the engine writes: `<expr>.<name>(<argument names>)`, `.await` for an
//! `async fn`, and `New::<name>(..)` for an associated function. What cannot forward (a pattern
//! parameter, a `const fn`, an associated const) is refused before anything is written.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{applying_a_plan_of, assert_compiles, AFixtureWorkspace};
use same_crate::{an_app_holding, blocks, the_anchor_over, the_impl_blocks_of};
use tddy_code_restructuring::RefactorOp;

const A_ROSTER: &str = "pub struct Roster {\n    pub(crate) n: u32,\n}\n";

/// A `host` module whose `Host` can reach a `Roster` through `self.roster()`, and holds `members` in
/// an `impl` of their own after the one that holds `roster`.
fn a_host_over_a_roster_holding(members: &str) -> String {
    format!(
        concat!(
            "use crate::roster::Roster;\n\n",
            "pub struct Host {{\n    pub(crate) n: u32,\n}}\n\n",
            "impl Host {{\n    pub fn roster(&self) -> Roster {{\n        Roster {{ n: self.n }}\n    }}\n}}\n\n",
            "impl Host {{\n{members}}}\n",
        ),
        members = members
    )
}

const A_CALLER_OF_GET: &str =
    "use crate::host::Host;\n\npub fn read(h: &Host) -> u32 {\n    h.get()\n}\n";
const A_CALLER_OF_NOTHING: &str =
    "use crate::host::Host;\n\npub fn read(h: &Host) -> u32 {\n    h.n\n}\n";

fn a_crate_whose_host_holds(members: &str) -> AFixtureWorkspace {
    a_crate_whose_host_holds_and_whose_caller_reads(members, A_CALLER_OF_NOTHING)
}

fn a_crate_whose_host_holds_and_whose_caller_reads(
    members: &str,
    caller: &str,
) -> AFixtureWorkspace {
    an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod caller;\npub mod host;\npub mod roster;\n",
        ),
        ("src/host.rs", &a_host_over_a_roster_holding(members)),
        ("src/roster.rs", A_ROSTER),
        ("src/caller.rs", caller),
    ])
}

/// The operation: retarget `members` of `src/host.rs` to `Roster`, leaving a delegator that reaches
/// the new type through `self.roster()`.
async fn retargeting_with_a_delegator(
    workspace: &AFixtureWorkspace,
    members: &[&str],
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let paths: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let anchor = the_anchor_over(workspace, "src/host.rs", &paths).await;
    let op: RefactorOp = serde_json::from_value(serde_json::json!({
        "op": "retarget_impl",
        "anchor": anchor,
        "to_type": "app::roster::Roster",
        "variant": "leave_delegator",
        "expr": "self.roster()",
    }))
    .expect("a delegating `retarget_impl` operation parses");
    applying_a_plan_of(workspace, &[op]).await
}

const A_READER: &str = "    pub fn get(&self) -> u32 {\n        self.n\n    }\n";

#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_forwarding_method_on_the_old_type_so_callers_keep_compiling() {
    // Given a caller of `Host::get` in another file, which is not in the plan
    let workspace = a_crate_whose_host_holds_and_whose_caller_reads(A_READER, A_CALLER_OF_GET);
    let caller_before = workspace.read("src/caller.rs");

    // When `get` is retargeted to `Roster` with a delegator reaching it through `self.roster()`
    retargeting_with_a_delegator(&workspace, &["Host::get"])
        .await
        .expect("the retarget applies");

    // Then `Host` keeps a method that forwards, `Roster` holds the original, the caller is untouched and the tree compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains("pub fn get(&self) -> u32 {\n        self.roster().get()\n    }\n"),
        "the old type kept no forwarding method:\n{host}"
    );
    assert_eq!(
        the_impl_blocks_of(&host),
        blocks(&[("Host", "roster"), ("Host", "get"), ("Roster", "get")])
    );
    assert_eq!(
        workspace.read("src/caller.rs"),
        caller_before,
        "the caller was edited"
    );
    assert_compiles(&workspace);
}

const AN_ASYNC_METHOD_AND_AN_ASSOCIATED_FUNCTION: &str = concat!(
    "    pub fn get(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    pub async fn load(&self) -> u32 {\n        self.n\n    }\n",
    "\n",
    "    pub fn make(n: u32) -> u32 {\n        n\n    }\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn forwards_an_async_method_with_await_and_an_associated_function_through_the_new_type() {
    // Given an `async` method and an associated function with no receiver
    let workspace = a_crate_whose_host_holds(AN_ASYNC_METHOD_AND_AN_ASSOCIATED_FUNCTION);

    // When both are retargeted with a delegator
    retargeting_with_a_delegator(&workspace, &["Host::load", "Host::make"])
        .await
        .expect("the retarget applies");

    // Then the method forwards with `.await`, the function through `Roster::`, and the tree compiles
    let host = workspace.read("src/host.rs");
    assert!(
        host.contains(
            "pub async fn load(&self) -> u32 {\n        self.roster().load().await\n    }\n"
        ),
        "the async method did not forward with `.await`:\n{host}"
    );
    assert!(
        host.contains("pub fn make(n: u32) -> u32 {\n        Roster::make(n)\n    }\n"),
        "the associated function did not forward through the new type:\n{host}"
    );
    assert_compiles(&workspace);
}

const A_METHOD_TAKING_A_PATTERN: &str =
    "    pub fn pair(&self, (a, b): (u32, u32)) -> u32 {\n        a + b + self.n\n    }\n";
const A_CONST_FN: &str = "    pub const fn zero() -> u32 {\n        0\n    }\n";
const AN_ASSOCIATED_CONST: &str = "    pub const LIMIT: u32 = 10;\n";

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_delegator_for_a_member_whose_parameter_is_a_pattern_a_const_fn_and_an_associated_const(
) {
    // Given three crates, each holding one member a forwarding method cannot be written for
    let with_a_pattern = a_crate_whose_host_holds(A_METHOD_TAKING_A_PATTERN);
    let with_a_const_fn = a_crate_whose_host_holds(A_CONST_FN);
    let with_a_const = a_crate_whose_host_holds(AN_ASSOCIATED_CONST);

    // When each member is retargeted with a delegator
    let pattern = retargeting_with_a_delegator(&with_a_pattern, &["Host::pair"]).await;
    let const_fn = retargeting_with_a_delegator(&with_a_const_fn, &["Host::zero"]).await;
    let constant = retargeting_with_a_delegator(&with_a_const, &["Host::LIMIT"]).await;

    // Then each is refused as a seam, naming why it cannot forward
    let refusals = [pattern, const_fn, constant].map(|outcome| outcome.expect_err("refused"));
    assert!(
        refusals
            .iter()
            .all(|refusal| refusal.contains("this seam cannot be cut here:"))
            && refusals[0].contains("takes `(a, b)` as a pattern")
            && refusals[1].contains("`zero` is a `const fn`")
            && refusals[2].contains("`LIMIT` is an associated const or type"),
        "unexpected refusals: {refusals:?}"
    );
}
