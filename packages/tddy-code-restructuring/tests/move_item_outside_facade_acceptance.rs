//! `reexport: outside` — re-point every caller in the item's own crate, and leave a facade only for
//! what something **outside** that crate reaches.
//!
//! The two facade modes `move_item` and `reparent_module` already had are blunt for a crate that has
//! consumers: `glob`/`named` re-point nothing, so the crate's own files keep naming the module the item
//! just left, and `none` re-points everything, which edits the consumers' files and removes the old
//! public path from under them. Splitting a crate by topic needs both at once: its own modules name the
//! new home, and the public path a consumer depends on keeps resolving.
//!
//! The consumer is a second package in the workspace, so "outside the crate" is a fact the reference
//! set states and not a guess from a visibility keyword.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, AFixtureWorkspace};
use same_crate::{
    a_move_item_op, an_app_with_a_consumer, moving_items, reparenting_module, the_anchor_over,
};

const LIB: &str = "pub mod answers;\npub mod handler;\npub mod pairing;\n";
const AN_EMPTY_ANSWERS_MODULE: &str = "//! What a peer answered about a session.\n";
const PAIRING_WITH_TWO_PREDICATES: &str = concat!(
    "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
    "    code == 404\n",
    "}\n",
    "\n",
    "pub fn peer_refused(code: u32) -> bool {\n",
    "    code == 403\n",
    "}\n",
);
const A_HANDLER_IN_THE_SAME_CRATE: &str = concat!(
    "use crate::pairing::peer_has_no_such_session;\n",
    "\n",
    "pub fn handle(code: u32) -> bool {\n",
    "    peer_has_no_such_session(code)\n",
    "}\n",
);
const A_CONSUMER_IN_ANOTHER_CRATE: &str = concat!(
    "pub fn check(code: u32) -> bool {\n",
    "    app::pairing::peer_has_no_such_session(code)\n",
    "}\n",
);

/// `app` holds the predicates and a handler that imports one; `consumer` names the same predicate
/// through its public path.
fn an_app_whose_predicate_a_consumer_also_reaches() -> AFixtureWorkspace {
    an_app_with_a_consumer(
        &[
            ("src/lib.rs", LIB),
            ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
            ("src/pairing.rs", PAIRING_WITH_TWO_PREDICATES),
            ("src/handler.rs", A_HANDLER_IN_THE_SAME_CRATE),
        ],
        &[("src/lib.rs", A_CONSUMER_IN_ANOTHER_CRATE)],
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn re_points_a_caller_in_the_same_crate_and_leaves_the_consumer_to_a_facade() {
    // Given a predicate reached by a handler in its own crate and by a consumer in another
    let workspace = an_app_whose_predicate_a_consumer_also_reaches();

    // When it moves into `answers` with `reexport: outside`
    moving_items(
        &workspace,
        "app/src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        Some("outside"),
    )
    .await
    .expect("the move applies");

    // Then the handler names the new module, the consumer is byte for byte what it was, and the
    // workspace compiles because `pairing` still answers to the old public path
    assert!(
        workspace
            .read("app/src/handler.rs")
            .contains("answers::peer_has_no_such_session"),
        "the caller in the same crate still names `pairing`:\n{}",
        workspace.read("app/src/handler.rs")
    );
    assert_eq!(
        workspace.read("consumer/src/lib.rs"),
        A_CONSUMER_IN_ANOTHER_CRATE,
        "a caller in another crate was edited"
    );
    assert!(
        workspace.read("app/src/pairing.rs").contains("pub use"),
        "no facade was left for the consumer:\n{}",
        workspace.read("app/src/pairing.rs")
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_no_facade_when_nothing_outside_the_crate_reaches_the_item() {
    // Given a predicate that only its own crate's handler reaches
    let workspace = an_app_with_a_consumer(
        &[
            ("src/lib.rs", LIB),
            ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
            ("src/pairing.rs", PAIRING_WITH_TWO_PREDICATES),
            ("src/handler.rs", A_HANDLER_IN_THE_SAME_CRATE),
        ],
        &[("src/lib.rs", "pub fn nothing() {}\n")],
    );

    // When it moves into `answers` with `reexport: outside`
    moving_items(
        &workspace,
        "app/src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        Some("outside"),
    )
    .await
    .expect("the move applies");

    // Then no facade is left behind, because no path outside the crate needs one
    assert!(
        !workspace.read("app/src/pairing.rs").contains("pub use"),
        "a facade was left although nothing outside the crate reaches the item:\n{}",
        workspace.read("app/src/pairing.rs")
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_facade_only_for_the_items_an_outside_crate_reaches() {
    // Given two predicates, only one of which the consumer reaches
    let workspace = an_app_whose_predicate_a_consumer_also_reaches();

    // When both move into `answers` as one run
    moving_items(
        &workspace,
        "app/src/pairing.rs",
        &["peer_has_no_such_session", "peer_refused"],
        "app::answers",
        Some("outside"),
    )
    .await
    .expect("the move applies");

    // Then the facade names the one the consumer reaches and not the other
    let facade = workspace.read("app/src/pairing.rs");
    assert!(
        facade.contains("peer_has_no_such_session"),
        "the item the consumer reaches has no facade:\n{facade}"
    );
    assert!(
        !facade.contains("peer_refused"),
        "a facade names an item nothing outside the crate reaches:\n{facade}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn does_the_same_for_a_module_that_is_reparented() {
    // Given a module `attachments` under `host`, reached from `split` in its crate and from a consumer
    let workspace = an_app_with_a_consumer(
        &[
            ("src/lib.rs", "pub mod host;\npub mod split;\n"),
            ("src/host.rs", "pub mod attachments;\n"),
            (
                "src/host/attachments.rs",
                "pub fn materialize() -> u32 {\n    1\n}\n",
            ),
            (
                "src/split.rs",
                "pub fn start() -> u32 {\n    crate::host::attachments::materialize()\n}\n",
            ),
        ],
        &[(
            "src/lib.rs",
            "pub fn run() -> u32 {\n    app::host::attachments::materialize()\n}\n",
        )],
    );

    // When it is re-parented under `split` with `reexport: outside`
    reparenting_module(
        &workspace,
        "app/src/host.rs",
        "attachments",
        "app::split",
        Some("outside"),
    )
    .await
    .expect("the re-parent applies");

    // Then the crate's own caller names the new path, the consumer is untouched, and it compiles
    assert!(
        workspace
            .read("app/src/split.rs")
            .contains("attachments::materialize")
            && !workspace
                .read("app/src/split.rs")
                .contains("host::attachments"),
        "the caller in the same crate still names `host`:\n{}",
        workspace.read("app/src/split.rs")
    );
    assert!(
        workspace
            .read("consumer/src/lib.rs")
            .contains("app::host::attachments::materialize"),
        "a caller in another crate was edited"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn is_refused_where_the_operation_has_no_facade_to_leave() {
    // Given an `extract_module` plan that asks for the outside facade
    let workspace = an_app_with_a_consumer(
        &[
            ("src/lib.rs", LIB),
            ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
            ("src/pairing.rs", PAIRING_WITH_TWO_PREDICATES),
            ("src/handler.rs", A_HANDLER_IN_THE_SAME_CRATE),
        ],
        &[("src/lib.rs", "pub fn nothing() {}\n")],
    );
    let anchor = the_anchor_over(&workspace, "app/src/pairing.rs", &["peer_refused"]).await;
    let mut op = a_move_item_op(&anchor, "app::answers", None);
    op.op = tddy_code_restructuring::RefactorKind::ExtractModule;
    op.reexport = Some(tddy_code_restructuring::Reexport::Glob);
    op.name = Some("grouped".to_string());

    // When the plan is statically checked with `outside` written into it as text
    let plan = workspace.a_plan_of(&[op]);
    let text = std::fs::read_to_string(&plan)
        .expect("the plan")
        .replace("\"reexport\":\"glob\"", "\"reexport\":\"outside\"");
    std::fs::write(&plan, text).expect("the plan with `outside`");
    let findings = harness::checking_the_plan(&workspace, plan, false)
        .await
        .map(|found| found.join("\n"))
        .unwrap_or_else(|refusal| refusal);

    // Then it is refused, saying which operations the value belongs to
    assert!(
        findings.contains("outside")
            && findings.contains("move_item")
            && findings.contains("reparent_module"),
        "the refusal did not say `outside` belongs to `move_item` and `reparent_module`: {findings}"
    );
}
