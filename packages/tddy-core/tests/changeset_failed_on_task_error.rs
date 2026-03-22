//! When a flow step fails (backend error, missing submit, etc.), `changeset.yaml` must record
//! `state.current: Failed` so sessions do not appear stuck in `GreenImplementing` or similar.

use std::sync::Arc;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::graph::GraphBuilder;
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{FileSessionStorage, Session, SessionStorage};
use tddy_core::workflow::task::FailingTask;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;

#[tokio::test]
async fn changeset_persists_failed_when_runner_task_errors() {
    let dir = std::env::temp_dir().join("tddy-changeset-failed-on-error");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut cs = Changeset::default();
    cs.state.current = "GreenImplementing".to_string();
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
    session.context.set_sync("plan_dir", dir.clone());

    storage.save(&session).await.unwrap();

    let hooks = Arc::new(TddWorkflowHooks::new());
    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));
    let result = runner.run("s1").await;
    assert!(result.is_err());

    let cs = read_changeset(&dir).unwrap();
    assert_eq!(cs.state.current, "Failed");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}
