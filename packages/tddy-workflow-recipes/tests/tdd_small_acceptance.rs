//! Acceptance tests for `tdd-small` workflow recipe (PRD Testing Plan).
//!
//! These fail until `tdd-small` graph, merged submit schema, and recipe-specific merged-red prompts are implemented.

use std::collections::BTreeSet;
use std::sync::Arc;

use tddy_core::backend::StubBackend;
use tddy_core::workflow::context::Context;
use tddy_core::WorkflowRecipe;
use tddy_workflow_recipes::{
    merged_red_system_prompt, parse_post_green_review_response, PostGreenReviewOutput,
    TddSmallRecipe,
};

/// Golden JSON for merged post-green submit (`goal` string is provisional until finalized with `tddy-tools get-schema`).
const POST_GREEN_REVIEW_GOLDEN: &str = r#"{
  "goal": "post-green-review",
  "summary": "Merged evaluate and validate summary.",
  "risk_level": "medium",
  "validity_assessment": "Changes appear consistent with PRD.",
  "tests_report_written": true,
  "prod_ready_report_written": false,
  "clean_code_report_written": true
}"#;

/// PRD: graph has plan → red → … → end; no `demo`, no standalone `acceptance-tests`, no separate evaluate/validate tasks.
#[test]
fn tdd_small_graph_excludes_demo_and_acceptance_tests_nodes() {
    let backend = Arc::new(StubBackend::new());
    let graph = TddSmallRecipe.build_graph(backend);

    assert_eq!(graph.id, "tdd_small_workflow");

    let ids: BTreeSet<String> = graph.task_ids().cloned().collect();
    let expected: BTreeSet<String> = [
        "plan",
        "red",
        "green",
        "post-green-review",
        "refactor",
        "update-docs",
        "end",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(
        ids, expected,
        "tdd-small must expose exactly this task id set (merged red; single post-green step)"
    );

    assert!(
        !ids.contains("demo"),
        "tdd-small must not include demo task id"
    );
    assert!(
        !ids.contains("acceptance-tests"),
        "tdd-small must not include standalone acceptance-tests task id"
    );
    assert!(
        !ids.contains("evaluate") && !ids.contains("validate"),
        "tdd-small must not include separate evaluate/validate task ids"
    );

    let ctx = Context::new();
    assert_eq!(
        graph.next_task_id("plan", &ctx),
        Some("red".to_string()),
        "plan must edge directly to merged red"
    );
    assert_eq!(
        graph.next_task_id("green", &ctx),
        Some("post-green-review".to_string()),
        "green must have a single successor: merged post-green step"
    );
}

/// PRD: merged evaluate+validate JSON round-trips without losing required fields.
#[test]
fn tdd_small_merged_submit_schema_round_trip() {
    let parsed: PostGreenReviewOutput =
        parse_post_green_review_response(POST_GREEN_REVIEW_GOLDEN).expect("parse merged submit");

    assert_eq!(parsed.goal, "post-green-review");
    assert!(!parsed.summary.is_empty());
    assert_eq!(parsed.risk_level, "medium");
    assert!(!parsed.validity_assessment.is_empty());

    let again = serde_json::to_string(&parsed).expect("serialize");
    let back: PostGreenReviewOutput = serde_json::from_str(&again).expect("deserialize");
    assert_eq!(parsed, back);
}

/// PRD: merged `red` prompts and single post-green submit path are recipe-specific (not verbatim classic `tdd` red).
#[test]
fn tdd_small_hooks_merged_red_and_single_submit() {
    let merged = merged_red_system_prompt();
    assert!(
        merged.contains("tdd-small merged red"),
        "merged red system prompt must identify the tdd-small recipe (not alias classic tdd red text)"
    );

    parse_post_green_review_response(POST_GREEN_REVIEW_GOLDEN)
        .expect("post-green single submit must parse");
}
