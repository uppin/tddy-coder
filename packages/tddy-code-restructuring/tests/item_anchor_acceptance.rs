//! Item anchors, against a live rust-analyzer: a crate-rooted item path plus a range relative to
//! the item, resolved through the server's outline at run open.
//!
//! The point of an item anchor is what the *server's* outline says, so a double would test
//! nothing. Every test here builds a real crate, resolves or applies through the runner, and
//! asserts on the tree — the claim is that the anchor lands where a correct range would, whatever
//! happened to the lines above it.
//!
//! Load-sensitive: one server at a time, enforced by the harness.

mod harness;

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, a_sink_that_keeps_what_it_hears,
    an_extract_method_at, an_item_anchor, applying_a_plan_of, applying_the_plan_at,
    applying_the_plan_with, at, checking_the_plan, resolving_the_item, QUEUE_NEW, STACK_NEW,
    WORKFLOW,
};
use tddy_code_restructuring::{Anchor, Position};

/// The two `let` lines of `Stack::new`: lines 2–3 of the item, from the `let` to the `;`.
fn the_lets_of_stack_new() -> Option<(Position, Position)> {
    Some((at(2, 9), at(3, 33)))
}

fn an_extraction_of_stack_news_lets(hint: Option<Position>) -> tddy_code_restructuring::RefactorOp {
    an_extract_method_at(
        an_item_anchor(
            WORKFLOW,
            "stacks::workflow::Stack::new",
            STACK_NEW,
            the_lets_of_stack_new(),
            hint,
        ),
        "fresh_items",
    )
}

/// What `Stack::new` reads after its two `let` lines are extracted, wherever it sits in the file.
const STACK_NEW_AFTER_THE_EXTRACTION: &str = "fresh_items()";

#[tokio::test(flavor = "multi_thread")]
async fn an_item_anchor_resolves_exactly_after_lines_are_inserted_above_its_item() {
    // Given a plan anchored in `Stack::new`, and three lines inserted above `Stack` afterwards
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(Some(at(8, 9)))]);
    let original = workspace.read(WORKFLOW);
    workspace.rewriting(WORKFLOW, &format!("// one\n// two\n// three\n{original}"));

    // When the plan is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then the extraction took `Stack::new`'s own lines, and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    assert!(
        workspace
            .read(WORKFLOW)
            .contains(STACK_NEW_AFTER_THE_EXTRACTION),
        "Stack::new did not have its lets extracted:\n{}",
        workspace.read(WORKFLOW)
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_anchor_ignores_a_wrong_hint() {
    // Given an item anchor whose absolute hint points at `Queue::new` instead
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let op = an_extraction_of_stack_news_lets(Some(at(21, 9)));

    // When it is applied
    let summary = applying_a_plan_of(&workspace, &[op]).await;

    // Then `Stack::new` is the one that changed; `Queue::new` is untouched
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    assert!(workspace.read(WORKFLOW).contains(QUEUE_NEW));
    assert!(!workspace.read(WORKFLOW).contains(STACK_NEW));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_inherent_impls_resolve_their_own_new() {
    // Given one file where both `Stack` and `Queue` define an inherent `new`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();

    // When each is resolved by its path
    let stack = resolving_the_item(&workspace, WORKFLOW, "stacks::workflow::Stack::new").await;
    let queue = resolving_the_item(&workspace, WORKFLOW, "stacks::workflow::Queue::new").await;

    // Then each lands on its own method's lines
    assert_eq!(
        stack.map(|item| (item.range.start.line, item.range.end.line)),
        Ok((7, 11))
    );
    assert_eq!(
        queue.map(|item| (item.range.start.line, item.range.end.line)),
        Ok((19, 22))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trait_member_collision_is_refused_until_the_trait_is_named() {
    // Given `Stack` with both `Display::fmt` and `Debug::fmt`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();

    // When `fmt` is named without its trait, and then with it
    let ambiguous = resolving_the_item(&workspace, WORKFLOW, "stacks::workflow::Stack::fmt").await;
    let qualified = resolving_the_item(
        &workspace,
        WORKFLOW,
        "stacks::workflow::<Stack as Debug>::fmt",
    )
    .await;

    // Then the bare name is refused naming the ambiguity, and the qualified one resolves to `Debug`'s
    assert_eq!(
        ambiguous,
        Err(
            "plan is malformed: `stacks::workflow::Stack::fmt` names 2 items in \
             crates/stacks/src/workflow.rs — qualify it with the trait, as \
             `stacks::workflow::<Stack as Trait>::fmt`"
                .to_string()
        )
    );
    assert_eq!(qualified.map(|item| item.range.start.line), Ok(32));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_inside_the_anchored_item_is_refused_naming_the_item() {
    // Given a plan anchored in `Stack::new`, and `Stack::new` edited afterwards
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(None)]);
    let edited = workspace
        .read(WORKFLOW)
        .replace("let depth = items.len();", "let depth = items.len() + 1;");
    workspace.rewriting(WORKFLOW, &edited);

    // When the plan is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then it is refused naming the item, and nothing is written
    assert_eq!(
        summary.map(|run| run.applied),
        Err(format!(
            "the item `stacks::workflow::Stack::new` in {WORKFLOW} changed since the plan was \
             written — its fingerprint no longer matches; re-anchor it with `restructure anchors`"
        ))
    );
    assert_eq!(workspace.read(WORKFLOW), edited);
    assert!(
        !workspace.holds(".restructure"),
        "a run refused for a changed item left its state directory behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_outside_the_anchored_item_is_not_refused() {
    // Given a plan anchored in `Stack::new`, and `Queue::new` edited afterwards
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(None)]);
    let edited = workspace
        .read(WORKFLOW)
        .replace("Vec::with_capacity(4)", "Vec::with_capacity(8)");
    workspace.rewriting(WORKFLOW, &edited);

    // When the plan is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then it applies: `Stack::new` lost its lets, and `Queue::new` keeps the edit made to it
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let after = workspace.read(WORKFLOW);
    assert!(
        after.contains(STACK_NEW_AFTER_THE_EXTRACTION),
        "Stack::new did not have its lets extracted:\n{after}"
    );
    assert!(
        after.contains("Vec::with_capacity(8)"),
        "the edit outside the anchored item was lost:\n{after}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_absent_from_its_file_is_refused_without_searching_elsewhere() {
    // Given an item path naming a type the file does not declare
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();

    // When it is resolved
    let resolved = resolving_the_item(&workspace, WORKFLOW, "stacks::workflow::Deque::new").await;

    // Then the missing segment is named
    assert_eq!(
        resolved,
        Err(format!(
            "plan is malformed: `stacks::workflow::Deque::new` is not declared in {WORKFLOW}: \
             nothing there is named `Deque`"
        ))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_prefix_that_does_not_match_the_file_is_refused() {
    // Given an item path whose module is not the file's
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();

    // When it is resolved against `workflow.rs`
    let resolved = resolving_the_item(&workspace, WORKFLOW, "stacks::planning::Stack::new").await;

    // Then the prefix mismatch is named
    assert_eq!(
        resolved,
        Err(format!(
            "plan is malformed: `stacks::planning::Stack::new` is not in {WORKFLOW}, which is \
             module `stacks::workflow`"
        ))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_relative_range_outside_its_item_is_refused_as_malformed() {
    // Given an anchor whose relative range reaches line 9 of a five-line item
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let op = an_extract_method_at(
        an_item_anchor(
            WORKFLOW,
            "stacks::workflow::Stack::new",
            STACK_NEW,
            Some((at(2, 9), at(9, 1))),
            None,
        ),
        "fresh_items",
    );

    // When it is applied
    let summary = applying_a_plan_of(&workspace, &[op]).await;

    // Then it is refused as malformed, naming the item's extent
    assert_eq!(
        summary.map(|run| run.applied),
        Err("plan is malformed: the range 2:9–9:1 reaches outside \
             `stacks::workflow::Stack::new`, which is 5 line(s) long"
            .to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extract_method_through_an_item_anchor_matches_the_range_anchor_edit() {
    // Given two copies of one crate: one extraction by item anchor, one by the equivalent range
    let by_item = a_crate_with_two_inherent_news_and_two_fmts();
    let by_range = a_crate_with_two_inherent_news_and_two_fmts();
    let range_op = an_extract_method_at(
        Anchor::Range {
            file: WORKFLOW.to_string(),
            start: at(8, 9),
            end: at(9, 33),
        },
        "fresh_items",
    );

    // When both are applied
    let item_summary =
        applying_a_plan_of(&by_item, &[an_extraction_of_stack_news_lets(None)]).await;
    let range_summary = applying_a_plan_of(&by_range, &[range_op]).await;

    // Then the two trees are identical
    assert_eq!(item_summary.map(|run| run.applied), Ok(1));
    assert_eq!(range_summary.map(|run| run.applied), Ok(1));
    assert_eq!(by_item.read(WORKFLOW), by_range.read(WORKFLOW));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_v2_plan_with_unrelated_file_drift_runs() {
    // Given a v2 plan, and an unrelated line appended to the file afterwards
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(None)]);
    let drifted = format!(
        "{}\npub const UNRELATED: u32 = 1;\n",
        workspace.read(WORKFLOW)
    );
    workspace.rewriting(WORKFLOW, &drifted);

    // When it is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then the drifted hint does not refuse it, and the extraction landed in `Stack::new`
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let after = workspace.read(WORKFLOW);
    assert!(
        after.contains(STACK_NEW_AFTER_THE_EXTRACTION),
        "Stack::new did not have its lets extracted:\n{after}"
    );
    assert!(
        after.contains("pub const UNRELATED: u32 = 1;"),
        "the unrelated line was lost:\n{after}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_v1_plan_with_the_same_drift_is_still_refused() {
    // Given a v1 range plan, and the same unrelated line appended afterwards
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_plan_of(&[an_extract_method_at(
        Anchor::Range {
            file: WORKFLOW.to_string(),
            start: at(8, 9),
            end: at(9, 33),
        },
        "fresh_items",
    )]);
    let drifted = format!(
        "{}\npub const UNRELATED: u32 = 1;\n",
        workspace.read(WORKFLOW)
    );
    workspace.rewriting(WORKFLOW, &drifted);

    // When it is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then v1's snapshot refusal still holds — a prefix, because the rest of the message is two
    // content hashes this test has no reason to restate
    assert!(
        summary
            .expect_err("a v1 plan over a drifted file is refused")
            .starts_with(&format!("snapshot mismatch for {WORKFLOW}")),
        "the refusal was not the snapshot mismatch"
    );
}

fn an_extraction_of_queue_news_first_let() -> tddy_code_restructuring::RefactorOp {
    an_extract_method_at(
        an_item_anchor(
            WORKFLOW,
            "stacks::workflow::Queue::new",
            QUEUE_NEW,
            Some((at(2, 9), at(2, 43))),
            None,
        ),
        "fresh_queue_items",
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hinted_file_the_tree_has_since_lost_is_reported_as_drift_and_the_plan_runs() {
    // Given a v2 plan that also hints at a file which has since been deleted
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of_a_tree_that_since_lost(
        &[an_extraction_of_stack_news_lets(None)],
        "crates/stacks/src/gone.rs",
    );
    let (progress, heard) = a_sink_that_keeps_what_it_hears();

    // When it is applied
    let summary =
        applying_the_plan_with(&workspace, plan, |options| options.progress = progress).await;

    // Then it runs, and said the file had drifted rather than refusing over it
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let heard = heard.lock().expect("the lines are readable").join("\n");
    assert!(
        heard.contains(
            "crates/stacks/src/gone.rs has changed since the plan was written; item anchors do \
             not depend on it"
        ),
        "the missing hinted file was not reported as drift:\n{heard}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_run_of_a_plan_that_anchors_by_item_is_refused_as_what_it_is() {
    // Given a two-operation item-anchored plan, stopped after its first operation
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[
        an_extraction_of_stack_news_lets(None),
        an_extraction_of_queue_news_first_let(),
    ]);
    let first = applying_the_plan_with(&workspace, plan.clone(), |options| {
        options.stop_after = Some(1);
    })
    .await;
    assert_eq!(first.map(|run| run.applied), Ok(1));

    // When the remainder is resumed
    let resumed = applying_the_plan_with(&workspace, plan, |options| options.resume = true).await;

    // Then it is refused for what it is — a continued run — and not as a malformed plan
    let refusal = resumed.expect_err("a continued run of an item-anchored plan is refused");
    assert!(
        refusal.starts_with("this run continues a journal that already applied 1 operation(s)"),
        "the refusal was not the continued-run one:\n{refusal}"
    );
    assert!(!refusal.contains("malformed"), "{refusal}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_static_check_of_an_item_anchored_plan_is_not_green_because_it_examined_nothing() {
    // Given a plan anchored by item
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(None)]);

    // When it is checked without a server
    let findings = checking_the_plan(&workspace, plan, false).await;

    // Then the operation is a finding, saying only a deep check can examine it
    let findings = findings.expect("a static check of a parseable plan reports");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].contains("anchors by item") && findings[0].contains("check --deep"),
        "{findings:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_resolves_an_item_anchor_and_finds_a_sound_plan_sound() {
    // Given a plan anchored by item over an untouched tree
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = workspace.a_hinted_plan_of(&[an_extraction_of_stack_news_lets(None)]);

    // When it is checked deeply
    let findings = checking_the_plan(&workspace, plan, true).await;

    // Then the anchor resolved and nothing is found
    assert_eq!(findings, Ok(Vec::new()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_refuses_an_item_anchor_whose_item_is_not_in_its_file() {
    // Given a plan anchored in an item the file does not declare
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let op = an_extract_method_at(
        an_item_anchor(
            WORKFLOW,
            "stacks::workflow::Deque::new",
            STACK_NEW,
            Some((at(2, 9), at(3, 33))),
            None,
        ),
        "fresh_items",
    );
    let plan = workspace.a_hinted_plan_of(&[op]);

    // When it is checked deeply
    let findings = checking_the_plan(&workspace, plan, true).await;

    // Then resolution named the missing item, which only a resolving check could have
    assert_eq!(
        findings,
        Err(format!(
            "plan is malformed: `stacks::workflow::Deque::new` is not declared in {WORKFLOW}: \
             nothing there is named `Deque`"
        ))
    );
}
