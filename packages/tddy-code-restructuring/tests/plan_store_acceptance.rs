//! The plan store: a plan is read once, its operations are given ids, the executor runs what the
//! store holds, and the store writes the plan back — without ever overwriting a file somebody
//! changed underneath it.
//!
//! Most of these need no language server: loading, ids and flushing are bookkeeping over files.
//! The two that apply run against a live rust-analyzer through the runner, because "executes what
//! was loaded" and "a one-shot apply still flushes" are claims about the apply path itself.

mod harness;

use std::path::{Path, PathBuf};
use std::time::Duration;

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, an_empty_fixture, applying_from_the_store,
    applying_the_plan_at, WORKFLOW,
};
use tddy_code_restructuring::plan_store::{FlushPolicy, PlanStore};
use tddy_code_restructuring::Plan;

const A_RENAME: &str =
    r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#;
const ANOTHER_RENAME: &str =
    r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"C"},"name":"D"}"#;

/// Extract `Stack::new`'s two `let` lines (absolute 8–9) into `fresh_items`.
const EXTRACT_STACKS_LETS: &str = r#"{"op":"extract_method","anchor":{"kind":"range","file":"crates/stacks/src/workflow.rs","start":{"line":8,"col":9},"end":{"line":9,"col":33}},"name":"fresh_items"}"#;

/// Extract `Queue::new`'s one `let` line (absolute 20) into `sized_items`.
const EXTRACT_QUEUES_LET: &str = r#"{"op":"extract_method","anchor":{"kind":"range","file":"crates/stacks/src/workflow.rs","start":{"line":20,"col":9},"end":{"line":20,"col":43}},"name":"sized_items"}"#;

fn a_plan_file(root: &Path, name: &str, ops: &[&str]) -> PathBuf {
    let mut lines = vec![r#"{"v":1,"snapshot":{}}"#.to_string()];
    lines.extend(ops.iter().map(|op| op.to_string()));
    let path = root.join(name);
    std::fs::write(&path, lines.join("\n") + "\n").expect("the plan is written");
    path
}

fn a_store_at(root: &Path) -> PlanStore {
    PlanStore::new(
        root,
        FlushPolicy {
            debounce: Duration::from_secs(3600),
        },
    )
}

fn the_plan_on_disk(path: &Path) -> Plan {
    Plan::parse(&std::fs::read_to_string(path).expect("the plan reads")).expect("the plan parses")
}

#[test]
fn loading_a_plan_without_ids_assigns_them_and_the_flush_writes_them() {
    // Given a plan of two operations, neither carrying an id
    let workspace = an_empty_fixture();
    let plan = a_plan_file(workspace.path(), "plan.jsonl", &[A_RENAME, ANOTHER_RENAME]);
    let mut store = a_store_at(workspace.path());

    // When it is loaded and flushed
    store
        .load(&[PathBuf::from("plan.jsonl")])
        .expect("the plan loads");
    store.flush_all().expect("the plan flushes");

    // Then both operations carry an id on disk, and the two differ — the values themselves are the
    // store's to choose, so what is pinned is that they exist and are distinct
    let ids: Vec<_> = the_plan_on_disk(&plan)
        .ops
        .into_iter()
        .map(|op| op.id.expect("every operation carries an id"))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn two_ops_sharing_an_id_are_refused_as_malformed() {
    // Given a plan whose two operations carry one id
    let workspace = an_empty_fixture();
    a_plan_file(
        workspace.path(),
        "plan.jsonl",
        &[
            &A_RENAME.replacen('{', r#"{"id":"same","#, 1),
            &ANOTHER_RENAME.replacen('{', r#"{"id":"same","#, 1),
        ],
    );
    let mut store = a_store_at(workspace.path());

    // When it is loaded
    let loaded = store.load(&[PathBuf::from("plan.jsonl")]);

    // Then it is refused, naming the id
    assert_eq!(
        loaded.map(|_| ()).map_err(|error| error.to_string()),
        Err("plan is malformed: plan.jsonl: two operations share the id `same`".to_string())
    );
}

#[test]
fn a_flush_onto_a_plan_changed_on_disk_is_refused_and_leaves_the_file() {
    // Given a loaded plan made dirty by id assignment, and its file edited by hand afterwards
    let workspace = an_empty_fixture();
    let plan = a_plan_file(workspace.path(), "plan.jsonl", &[A_RENAME]);
    let mut store = a_store_at(workspace.path());
    store
        .load(&[PathBuf::from("plan.jsonl")])
        .expect("the plan loads");
    let edited = format!(
        "{}{ANOTHER_RENAME}\n",
        std::fs::read_to_string(&plan).unwrap()
    );
    std::fs::write(&plan, &edited).unwrap();

    // When the store flushes
    let flushed = store.flush_all();

    // Then it refuses naming the plan, and the hand edit survives
    assert_eq!(
        flushed.map(|_| ()).map_err(|error| error.to_string()),
        Err(
            "plan.jsonl changed on disk since it was loaded — not overwriting it; unload it and \
             load it again"
                .to_string()
        )
    );
    assert_eq!(std::fs::read_to_string(&plan).unwrap(), edited);
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_executes_the_loaded_ops_not_the_edited_file() {
    // Given a loaded plan extracting from `Stack::new`, and its file then rewritten to extract from
    // `Queue::new` instead
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_file(workspace.path(), "plan.jsonl", &[EXTRACT_STACKS_LETS]);
    let mut store = a_store_at(workspace.path());
    store
        .load(&[PathBuf::from("plan.jsonl")])
        .expect("the plan loads");
    let key = store.key_for(Path::new("plan.jsonl")).unwrap();
    std::fs::write(
        &plan,
        format!("{{\"v\":1,\"snapshot\":{{}}}}\n{EXTRACT_QUEUES_LET}\n"),
    )
    .unwrap();

    // When the plan is applied from the store
    let (_store, summary) = applying_from_the_store(&workspace, store, key).await;

    // Then `Stack::new` was extracted and `Queue::new` was not; the final flush refuses to
    // overwrite the rewritten file
    assert!(workspace.read(WORKFLOW).contains("fn fresh_items"));
    assert!(!workspace.read(WORKFLOW).contains("fn sized_items"));
    assert_eq!(
        summary.map(|run| run.applied),
        Err(
            "plan.jsonl changed on disk since it was loaded — not overwriting it; unload it and \
             load it again"
                .to_string()
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_without_a_daemon_still_flushes_the_plan_at_exit() {
    // Given a plan whose one operation carries no id
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let plan = a_plan_file(workspace.path(), "plan.jsonl", &[EXTRACT_STACKS_LETS]);

    // When it is applied in process, the way `tddy-tools restructure apply` runs with no daemon
    let summary = applying_the_plan_at(&workspace, plan.clone()).await;

    // Then it applied, and the file it came from now carries the operation's id
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    assert!(the_plan_on_disk(&plan).ops[0].id.is_some());
}
