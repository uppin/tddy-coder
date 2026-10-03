//! A plan the store has run part-way describes the tree as it now is, so what resumes it works.
//!
//! Both claims need a live rust-analyzer: an extraction's inserted lines are the server's to
//! choose, so "the second operation moved" and "the resumed run still lands on its text" cannot be
//! stated against a stub.

mod harness;

use std::path::{Path, PathBuf};

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, applying_the_plan_with, AFixtureWorkspace,
    WORKFLOW,
};
use tddy_code_restructuring::{Anchor, Plan};

/// Extract `Stack::new`'s two `let` lines (absolute 8–9) into `fresh_items`, which adds a function
/// above `Queue`.
const EXTRACT_STACKS_LETS: &str = r#"{"op":"extract_method","anchor":{"kind":"range","file":"crates/stacks/src/workflow.rs","start":{"line":8,"col":9},"end":{"line":9,"col":33}},"name":"fresh_items"}"#;

/// Extract `Queue::new`'s one `let` line (absolute 20, before any edit) into `sized_items`.
const EXTRACT_QUEUES_LET: &str = r#"{"op":"extract_method","anchor":{"kind":"range","file":"crates/stacks/src/workflow.rs","start":{"line":20,"col":9},"end":{"line":20,"col":43}},"name":"sized_items"}"#;

fn a_plan_file(root: &Path, ops: &[&str]) -> PathBuf {
    let mut lines = vec![r#"{"v":1,"snapshot":{}}"#.to_string()];
    lines.extend(ops.iter().map(|op| op.to_string()));
    let path = root.join("plan.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").expect("the plan is written");
    path
}

/// A two-operation plan whose first operation has been applied and whose second has not.
async fn a_plan_stopped_after_its_first_operation(workspace: &AFixtureWorkspace) -> PathBuf {
    let plan = a_plan_file(workspace.path(), &[EXTRACT_STACKS_LETS, EXTRACT_QUEUES_LET]);
    let first = applying_the_plan_with(workspace, plan.clone(), |options| {
        options.stop_after = Some(1);
    })
    .await;
    assert_eq!(first.map(|run| run.applied), Ok(1));
    plan
}

#[tokio::test(flavor = "multi_thread")]
async fn the_plan_written_back_after_its_first_operation_anchors_the_second_below_where_it_was() {
    // Given a plan stopped after the extraction that adds a function above `Queue::new`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_stopped_after_its_first_operation(&workspace).await;

    // When the file is read back
    let written = Plan::parse(&std::fs::read_to_string(&plan).unwrap()).unwrap();

    // Then the second operation is anchored below line 20, where the original text put it
    let Anchor::Range { start, .. } = &written.ops[1].anchor else {
        panic!("the second operation lost its range anchor");
    };
    assert!(
        start.line > 20,
        "the anchor still reads line {}",
        start.line
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_run_applies_the_second_operation_where_its_text_now_is() {
    // Given a plan stopped after its first operation
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_stopped_after_its_first_operation(&workspace).await;

    // When the remainder is resumed from the plan file the first run wrote back
    let resumed = applying_the_plan_with(&workspace, plan, |options| options.resume = true).await;

    // Then the second extraction landed on `Queue::new`'s line, as a run that never stopped would
    assert_eq!(resumed.map(|run| run.applied), Ok(1));
    assert!(workspace.read(WORKFLOW).contains("fn sized_items"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_journal_from_before_plans_were_kept_current_still_resumes_a_plan_of_range_anchors() {
    // Given a stopped plan of ranges, as an older binary left it: the plan still in the
    // coordinates it started in, and a journal with no ids and no record of a write-back
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_stopped_after_its_first_operation(&workspace).await;
    a_plan_file(workspace.path(), &[EXTRACT_STACKS_LETS, EXTRACT_QUEUES_LET]);
    workspace.with_the_journal_of_before_plans_were_kept_current(&plan);

    // When the remainder is resumed
    let resumed = applying_the_plan_with(&workspace, plan, |options| options.resume = true).await;

    // Then the journal's own ledger carried the second operation to where its text now is
    assert_eq!(resumed.map(|run| run.applied), Ok(1));
    assert!(workspace.read(WORKFLOW).contains("fn sized_items"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_of_ranges_whose_pending_anchor_was_changed_after_the_run_wrote_it_is_refused() {
    // Given a stopped plan of ranges whose pending anchor was then moved by hand
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_stopped_after_its_first_operation(&workspace).await;
    let mut written = Plan::parse(&std::fs::read_to_string(&plan).unwrap()).unwrap();
    if let Anchor::Range { start, .. } = &mut written.ops[1].anchor {
        start.line += 1;
    }
    std::fs::write(&plan, written.to_jsonl()).unwrap();

    // When the remainder is resumed
    let resumed = applying_the_plan_with(&workspace, plan, |options| options.resume = true).await;

    // Then it is refused rather than applied at a position the tree does not hold
    assert_eq!(
        resumed.map(|run| run.applied).map_err(|refusal| refusal
            .starts_with("the plan's pending anchors do not match what the run recorded")),
        Err(true)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_stopped_between_an_operation_and_its_plan_record_is_refused_on_resume() {
    // Given a stopped plan whose journal lost the record that the plan was written back — a crash
    // after the operation was committed and before the record
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_stopped_after_its_first_operation(&workspace).await;
    let journal = tddy_code_restructuring::state_directory_for_plan(workspace.path(), &plan)
        .unwrap()
        .join("journal.jsonl");
    let committed_only: Vec<String> = std::fs::read_to_string(&journal)
        .unwrap()
        .lines()
        .filter(|record| !record.contains("\"plan_synced\""))
        .map(str::to_string)
        .collect();
    std::fs::write(&journal, committed_only.join("\n") + "\n").unwrap();

    // When the remainder is resumed
    let resumed = applying_the_plan_with(&workspace, plan, |options| options.resume = true).await;

    // Then it is refused, and the second operation was not applied
    assert_eq!(
        resumed.map(|run| run.applied).map_err(|refusal| refusal
            .starts_with("the plan's pending anchors do not match what the run recorded")),
        Err(true)
    );
    assert!(!workspace.read(WORKFLOW).contains("fn sized_items"));
}
