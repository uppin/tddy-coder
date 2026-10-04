//! `check --deep` accepts a well-formed same-crate move, and still reports a malformed one.
//!
//! `--deep` is the gate, not an option: it resolves every operation through the same path `apply`
//! uses, so it is the only form that reports an assist or import refusal and it writes nothing. For an
//! item-anchored operation that resolution **lowers** the anchor to a range first. An operation whose
//! own preflight reads the *anchor* (`move_item` and `reparent_module` take only `items`/`item`
//! anchors) then saw a range, and refused every plan with "names no module-level item" — a refusal
//! `apply --dry-run` did not share, so the gate rejected what the tool would have done.
//!
//! The acceptance suites for the two operations apply plans; none of them ran `check --deep`, which is
//! why nothing caught it before a real plan did.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::checking_the_plan;
use same_crate::{a_move_item_op, a_reparent_module_op, an_app_holding, the_anchor_over};

const LIB: &str =
    "pub mod answers;\npub mod handler;\npub mod host;\npub mod pairing;\npub mod split;\n";
const PAIRING: &str = "pub fn peer_has_no_such_session(code: u32) -> bool {\n    code == 404\n}\n";
const HANDLER: &str = concat!(
    "use crate::pairing::peer_has_no_such_session;\n",
    "\n",
    "pub fn handle(code: u32) -> bool {\n",
    "    peer_has_no_such_session(code)\n",
    "}\n",
);

fn a_crate_with_an_item_to_move_and_a_module_to_reparent() -> harness::AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/answers.rs", "//! Answers.\n"),
        ("src/pairing.rs", PAIRING),
        ("src/handler.rs", HANDLER),
        ("src/host.rs", "pub mod attachments;\n"),
        (
            "src/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    1\n}\n",
        ),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_finds_nothing_wrong_with_a_well_formed_move_item_plan() {
    // Given an item anchored the way the anchors command emits it, and a destination that exists
    let workspace = a_crate_with_an_item_to_move_and_a_module_to_reparent();
    let anchor = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;
    let plan = workspace.a_plan_of(&[a_move_item_op(&anchor, "app::answers", Some("outside"))]);

    // When the plan is checked deeply
    let findings = checking_the_plan(&workspace, plan, true)
        .await
        .expect("a deep check runs");

    // Then it finds nothing: the operation `apply` would perform is not refused
    assert!(
        findings.is_empty(),
        "a deep check refused a plan that applies: {findings:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_finds_nothing_wrong_with_a_well_formed_reparent_module_plan() {
    // Given a module anchored by its `mod` declaration, and a parent that exists
    let workspace = a_crate_with_an_item_to_move_and_a_module_to_reparent();
    let anchor = the_anchor_over(&workspace, "src/host.rs", &["attachments"]).await;
    let plan = workspace.a_plan_of(&[a_reparent_module_op(&anchor, "app::split", Some("outside"))]);

    // When the plan is checked deeply
    let findings = checking_the_plan(&workspace, plan, true)
        .await
        .expect("a deep check runs");

    // Then it finds nothing
    assert!(
        findings.is_empty(),
        "a deep check refused a plan that applies: {findings:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_still_reports_a_destination_that_does_not_exist() {
    // Given the same item and a destination that names no module
    let workspace = a_crate_with_an_item_to_move_and_a_module_to_reparent();
    let anchor = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;
    let plan = workspace.a_plan_of(&[a_move_item_op(&anchor, "app::nowhere", None)]);

    // When the plan is checked deeply
    let findings = checking_the_plan(&workspace, plan, true)
        .await
        .expect("a deep check runs");

    // Then the refusal that is true is reported, and names the destination
    assert!(
        findings.join("\n").contains("app::nowhere"),
        "a deep check did not report the missing destination: {findings:?}"
    );
}
