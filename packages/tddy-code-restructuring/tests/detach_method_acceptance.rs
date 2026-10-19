//! `detach_method` against a live rust-analyzer — `#reshape` 15/24.
//!
//! Every rule is decided at library level in `detach_method.rs`, against recorded server answers.
//! These two tests are the only place the real reference survey, receiver typing, the compile gate
//! and the runner meet: one detach whose result must compile with its callers, and one refusal
//! `check --deep` must make with the tree untouched. Capped at two (the 2026-10-17 rebalance rule:
//! each test here pays a rust-analyzer start).
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    applying_a_plan_of, assert_compiles_with_its_tests, assert_lints_clean,
    checking_deep_keeping_the_account, AFixtureWorkspace,
};
use same_crate::{an_app_holding, the_anchor_over};
use tddy_code_restructuring::{Anchor, RefactorOp};

/// A `detach_method` of the members `anchor` names, their receiver becoming `name`.
fn a_detach_of(anchor: &Anchor, name: &str) -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "detach_method",
        "anchor": anchor,
        "name": name,
    }))
    .expect("a `detach_method` operation deserializes")
}

/// The detach of `members` of `src/host.rs` (each `Host::bump`, as the `host` module names it)
/// over `session`, anchored as `restructure anchors --items` emits it.
async fn detaching(workspace: &AFixtureWorkspace, members: &[&str]) -> RefactorOp {
    let paths: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let anchor = the_anchor_over(workspace, "src/host.rs", &paths).await;
    a_detach_of(&anchor, "session")
}

const A_HOST_THAT_BUMPS: &str = concat!(
    "#[derive(Default)]\n",
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(super) fn bump(&mut self, by: u32) -> u32 {\n",
    "        self.n += by;\n",
    "        self.n\n",
    "    }\n",
    "\n",
    "    pub(crate) fn total(&self) -> u32 {\n",
    "        Self::step(self.n)\n",
    "    }\n",
    "\n",
    "    fn step(n: u32) -> u32 {\n",
    "        n + 1\n",
    "    }\n",
    "}\n",
);

const A_TICK_THAT_CALLS_THEM: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "pub fn tick(h: &mut Host) -> u32 {\n",
    "    let mut fresh = Host::default();\n",
    "    h.bump(2) + h.total() + fresh.bump(1)\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_detached_block_compiles_with_its_callers() {
    // Given a block of three members — `&mut self`, `&self` calling `Self::step`, an associated
    // function — called from the crate root and from a sibling module, on a reference and on a place
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod host;\npub mod tick;\n\npub fn run(h: &mut host::Host) -> u32 {\n    h.bump(1)\n}\n",
        ),
        ("src/host.rs", A_HOST_THAT_BUMPS),
        ("src/tick.rs", A_TICK_THAT_CALLS_THEM),
    ]);
    let op = detaching(&workspace, &["Host::bump", "Host::total", "Host::step"]).await;

    // When the whole block is detached over `session`
    applying_a_plan_of(&workspace, &[op])
        .await
        .expect("the detach applies");

    // Then nothing is left on the type, every caller calls a function, and the tree compiles with
    // its tests and lints clean
    let host = workspace.read("src/host.rs");
    let tick = workspace.read("src/tick.rs");
    assert!(
        !host.contains("impl Host {")
            && host.contains("pub(super) fn bump(session: &mut Host, by: u32) -> u32 {\n")
            && host
                .contains("pub(crate) fn total(session: &Host) -> u32 {\n    step(session.n)\n}\n"),
        "the block was not detached into functions:\n{host}"
    );
    assert!(
        tick.contains("use crate::host;\n")
            && tick.contains("    host::bump(h, 2) + host::total(h) + host::bump(&mut fresh, 1)\n"),
        "the calls in `tick` were not re-pointed:\n{tick}"
    );
    assert!(
        workspace
            .read("src/lib.rs")
            .contains("    host::bump(h, 1)\n"),
        "the call in the crate root was not re-pointed"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

const A_HOST_WITH_A_PUB_MEMBER: &str = concat!(
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub fn shown(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn check_deep_refuses_a_pub_member() {
    // Given a `pub` member, which callers outside the workspace may name
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod host;\n"),
        ("src/host.rs", A_HOST_WITH_A_PUB_MEMBER),
    ]);
    let op = detaching(&workspace, &["Host::shown"]).await;

    // When the detach is checked deep
    let (findings, _) = checking_deep_keeping_the_account(&workspace, &[op]).await;

    // Then it is refused at the member's line, and the tree is untouched
    let findings = findings.expect("the deep check runs");
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("src/host.rs:6")
                && finding
                    .contains("`Host::shown` is `pub` and may have callers outside the workspace")),
        "the deep check did not refuse the `pub` member: {findings:?}"
    );
    assert_eq!(workspace.read("src/host.rs"), A_HOST_WITH_A_PUB_MEMBER);
}
