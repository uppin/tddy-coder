//! When a flow step fails (backend error, missing submit, etc.), `changeset.yaml` must record
//! `state.current: Failed` so sessions do not appear stuck in `GreenImplementing` or similar.

mod common;

use std::sync::Arc;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::graph::GraphBuilder;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{FileSessionStorage, Session, SessionStorage};
use tddy_core::workflow::task::FailingTask;

use tddy_workflow_recipes::TddWorkflowHooks;

#[tokio::test]
async fn changeset_persists_failed_when_runner_task_errors() {
    let dir = std::env::temp_dir().join("tddy-changeset-failed-on-error");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut cs = Changeset::default();
    cs.state.current = WorkflowState::new("GreenImplementing");
    write_changeset(&dir, &cs).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-changeset-failed-storage");
    let _ = std::fs::remove_dir_all(&storage_dir);
    std::fs::create_dir_all(&storage_dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(storage_dir.clone()));

    let task = Arc::new(FailingTask::new("fail"));
    let graph = Arc::new(
        GraphBuilder::new("fail_cs")
            .add_task(task)
            .add_edge("fail", "fail")
            .build(),
    );

    let session =
        Session::new_from_task("s1".to_string(), "fail_cs".to_string(), "fail".to_string());
    session.context.set_sync("session_dir", dir.clone());

    storage.save(&session).await.unwrap();

    let hooks = Arc::new(TddWorkflowHooks::new(
        common::tdd_recipe(),
        common::tdd_manifest(),
    ));
    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));
    let result = runner.run("s1").await;
    assert!(result.is_err());

    let cs = read_changeset(&dir).unwrap();
    assert_eq!(cs.state.current.as_str(), "Failed");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}
