//! Every plan a store holds stays current as the tree moves: an operation applied from one plan is
//! folded into the others, a change made underneath the store is re-resolved, and an operation that
//! can no longer run as written is stale — reported, and refused before any write.
//!
//! Against a live rust-analyzer, through `runner::apply_from_store`, which is what the index daemon
//! runs for every loaded plan of a root.

mod harness;

use std::path::{Path, PathBuf};
use std::time::Duration;

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, applying_from_the_store,
    re_resolving_in_the_store, QUEUE_NEW, STACK_NEW, WORKFLOW,
};
use tddy_code_restructuring::plan_store::{
    FlushPolicy, OpStaleness, PlanKey, PlanStore, StaleReason,
};
use tddy_code_restructuring::{Fingerprint, OpId};

/// `first.jsonl`'s one op: extract `Stack::new`'s two `let` lines (absolute 8–9) — which inserts a
/// new function and so moves everything below `Stack::new`.
const FIRST: &str = r#"{"id":"a1","op":"extract_method","anchor":{"kind":"range","file":"crates/stacks/src/workflow.rs","start":{"line":8,"col":9},"end":{"line":9,"col":33}},"name":"fresh_items"}"#;

fn a_store_holding(
    workspace: &harness::AFixtureWorkspace,
    second: &str,
) -> (PlanStore, PlanKey, PlanKey) {
    for (name, op) in [
        ("first.jsonl", FIRST.to_string()),
        ("second.jsonl", second.to_string()),
    ] {
        std::fs::write(
            workspace.path().join(name),
            format!("{{\"v\":1,\"snapshot\":{{}}}}\n{op}\n"),
        )
        .expect("the plan is written");
    }
    let mut store = PlanStore::new(
        workspace.path(),
        FlushPolicy {
            debounce: Duration::from_secs(3600),
        },
    );
    store
        .load(&[PathBuf::from("first.jsonl"), PathBuf::from("second.jsonl")])
        .expect("both plans load");
    let first = store.key_for(Path::new("first.jsonl")).unwrap();
    let second = store.key_for(Path::new("second.jsonl")).unwrap();
    (store, first, second)
}

/// `second.jsonl`'s op, by item: extract `Queue::new`'s `let` line (line 2 of the item).
fn extracting_queues_let_by_item() -> String {
    format!(
        r#"{{"id":"b1","op":"extract_method","anchor":{{"kind":"item","item":"stacks::workflow::Queue::new","file":"{WORKFLOW}","start":{{"line":2,"col":9}},"end":{{"line":2,"col":43}},"fingerprint":"{}","hint":{{"line":20,"col":9}}}},"name":"sized_items"}}"#,
        Fingerprint::of(QUEUE_NEW).0
    )
}

/// `second.jsonl`'s op, by item, inside the lines `first.jsonl` extracts.
fn extracting_stacks_depth_line_by_item() -> String {
    format!(
        r#"{{"id":"b1","op":"extract_variable","anchor":{{"kind":"item","item":"stacks::workflow::Stack::new","file":"{WORKFLOW}","start":{{"line":3,"col":21}},"end":{{"line":3,"col":32}},"fingerprint":"{}","hint":{{"line":9,"col":21}}}},"name":"length"}}"#,
        Fingerprint::of(STACK_NEW).0
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn applying_plan_a_keeps_plan_bs_anchor_on_its_item() {
    // Given two held plans: `first` extracts from `Stack::new`, `second` from `Queue::new` below it
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let (store, first, second) = a_store_holding(&workspace, &extracting_queues_let_by_item());

    // When `first` is applied, and then `second`
    let (store, first_run) = applying_from_the_store(&workspace, store, first).await;
    let (_store, second_run) = applying_from_the_store(&workspace, store, second).await;

    // Then both applied, each in its own method, and the tree compiles
    assert_eq!(first_run.map(|run| run.applied), Ok(1));
    assert_eq!(second_run.map(|run| run.applied), Ok(1));
    assert!(workspace.read(WORKFLOW).contains("fn fresh_items"));
    assert!(workspace.read(WORKFLOW).contains("fn sized_items"));
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_inside_plan_bs_anchored_item_marks_its_op_stale_edited_by_plan_a() {
    // Given two held plans that both act on `Stack::new`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let (store, first, second) =
        a_store_holding(&workspace, &extracting_stacks_depth_line_by_item());

    // When `first` is applied
    let (store, first_run) = applying_from_the_store(&workspace, store, first.clone()).await;

    // Then `second`'s op is stale, naming the op that edited it
    assert_eq!(first_run.map(|run| run.applied), Ok(1));
    assert_eq!(
        store.stale_ops(&second),
        vec![OpStaleness {
            op: OpId("b1".to_string()),
            reason: StaleReason::EditedBy {
                plan: first,
                op: OpId("a1".to_string()),
            },
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_refuses_a_stale_next_op_before_any_write() {
    // Given `second`'s op made stale by `first`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let (store, first, second) =
        a_store_holding(&workspace, &extracting_stacks_depth_line_by_item());
    let (store, _) = applying_from_the_store(&workspace, store, first).await;
    let after_first = workspace.read(WORKFLOW);

    // When `second` is applied
    let (_store, second_run) = applying_from_the_store(&workspace, store, second).await;

    // Then it is refused naming the op and why, and the tree is as `first` left it
    assert_eq!(
        second_run.map(|run| run.applied),
        Err(
            "operation `b1` of second.jsonl is stale (edited by first.jsonl#a1) — re-anchor it \
             before applying"
                .to_string()
        )
    );
    assert_eq!(workspace.read(WORKFLOW), after_first);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_edit_above_the_item_refreshes_the_hint() {
    // Given a held plan anchored in `Queue::new`, and three lines written above `Stack` by hand
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let (store, _first, second) = a_store_holding(&workspace, &extracting_queues_let_by_item());
    let original = workspace.read(WORKFLOW);
    workspace.rewriting(WORKFLOW, &format!("// one\n// two\n// three\n{original}"));

    // When the store re-resolves the changed file
    let (store, outcome) =
        re_resolving_in_the_store(&workspace, store, vec![WORKFLOW.to_string()]).await;

    // Then the op's hint follows `Queue::new` down three lines, and it is not stale
    assert_eq!(outcome, Ok(()));
    let tddy_code_restructuring::Anchor::Item { hint, .. } =
        store.get(&second).unwrap().plan.ops[0].anchor.clone()
    else {
        panic!("the item anchor stayed an item anchor");
    };
    assert_eq!(hint, Some(harness::at(23, 9)));
    assert_eq!(store.stale_ops(&second), Vec::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_edit_inside_the_item_marks_the_op_stale() {
    // Given a held plan anchored in `Queue::new`, and `Queue::new` edited by hand
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let (store, _first, second) = a_store_holding(&workspace, &extracting_queues_let_by_item());
    let edited = workspace
        .read(WORKFLOW)
        .replace("Vec::with_capacity(4)", "Vec::with_capacity(8)");
    workspace.rewriting(WORKFLOW, &edited);

    // When the store re-resolves the changed file
    let (store, _) = re_resolving_in_the_store(&workspace, store, vec![WORKFLOW.to_string()]).await;

    // Then the op is stale because its item changed
    assert_eq!(
        store.stale_ops(&second),
        vec![OpStaleness {
            op: OpId("b1".to_string()),
            reason: StaleReason::ItemChanged,
        }]
    );
}

/// A plan no store holds, rebased once — `restructure snapshot` for an item-anchored plan.
fn a_plan_file_of(workspace: &harness::AFixtureWorkspace, op: &str) -> PathBuf {
    let plan = workspace.path().join("unheld.jsonl");
    std::fs::write(&plan, format!("{{\"v\":2,\"files\":{{}}}}\n{op}\n"))
        .expect("the plan is written");
    plan
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_re_resolves_item_anchors_after_lines_were_inserted_above_them() {
    // Given an item-anchored plan written before three lines were inserted above its item
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_file_of(&workspace, &extracting_queues_let_by_item());
    let original = workspace.read(WORKFLOW);
    workspace.rewriting(WORKFLOW, &format!("// one\n// two\n// three\n{original}"));

    // When the plan is rebased
    let stale = harness::rebasing_the_plan_file(&workspace, plan.clone()).await;

    // Then nothing is stale, and the written plan's hint follows the item
    assert_eq!(stale, Ok(Vec::new()));
    let written = tddy_code_restructuring::Plan::parse(&std::fs::read_to_string(&plan).unwrap())
        .expect("the rebased plan parses");
    let tddy_code_restructuring::Anchor::Item { hint, .. } = written.ops[0].anchor.clone() else {
        panic!("the item anchor stayed an item anchor");
    };
    assert_eq!(hint, Some(harness::at(23, 9)));
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_reports_an_op_whose_item_changed_and_leaves_it() {
    // Given an item-anchored plan, and its item edited since
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let op = extracting_queues_let_by_item();
    let plan = a_plan_file_of(&workspace, &op);
    let edited = workspace
        .read(WORKFLOW)
        .replace("Vec::with_capacity(4)", "Vec::with_capacity(8)");
    workspace.rewriting(WORKFLOW, &edited);

    // When the plan is rebased
    let stale = harness::rebasing_the_plan_file(&workspace, plan.clone()).await;

    // Then the op is reported, and its line is left exactly as written
    assert_eq!(
        stale,
        Ok(vec![OpStaleness {
            op: OpId("b1".to_string()),
            reason: StaleReason::ItemChanged,
        }])
    );
    assert!(std::fs::read_to_string(&plan).unwrap().contains(&op));
}
