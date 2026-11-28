//! Unit: deleting a work item removes its files directory — the queue mutation and the bytes go
//! together.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_core::session_work_queue_files::SessionWorkQueueFiles;
use tddy_core::work_queue_store::WorkQueueStore;
use tddy_tool_engine::work_queue_tools::WorkQueueOp;

use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;

use super::super::github::GithubSource;
use super::super::RealWorkQueueGithubClients;
use super::OpEnv;

const PROJECT: &str = "proj-delete-files";
const OWNER: &str = "queue-owner-delete";

fn an_env_with_a_deleted_item_pending() -> (tempfile::TempDir, OpEnv, PathBuf) {
    let data = tempfile::tempdir().expect("temp data dir");
    let sessions_dir = data.path().join("sessions");
    SessionWorkQueues::new(&sessions_dir)
        .create_for_session(OWNER, PROJECT, 0)
        .expect("the owner gets its queue");
    let store = WorkQueueStore::new(data.path());
    store
        .update(PROJECT, OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![
                    WorkItem {
                        node_id: "n1".to_string(),
                        title: "Keep me".to_string(),
                        ..WorkItem::default()
                    },
                    WorkItem {
                        node_id: "n2".to_string(),
                        title: "Delete me".to_string(),
                        ..WorkItem::default()
                    },
                ],
                ..WorkQueue::default()
            };
        })
        .expect("items added");

    // n2 carries a file; n1 does not.
    let files = SessionWorkQueueFiles::new(&sessions_dir);
    files
        .store(OWNER, "n2", "failure.log", b"# failure log\n")
        .expect("n2's file stored");

    let env = OpEnv {
        store,
        project_id: PROJECT.to_string(),
        queue_id: Some(OWNER.to_string()),
        sessions_base: data.path().to_path_buf(),
    };
    (data, env, sessions_dir)
}

fn github_source(_env: &OpEnv) -> GithubSource {
    GithubSource {
        projects_dir: PathBuf::from("/nonexistent-projects"),
        project_id: PROJECT.to_string(),
        os_user: "testdev".to_string(),
        identity: SessionAccountAccess::new(None, Arc::new(|_: &str| None), OWNER),
        github_cli: tddy_host_service::GithubCli::on_spawn_child_path(),
        clients: Arc::new(RealWorkQueueGithubClients),
    }
}

#[test]
fn deleting_an_item_removes_its_files_and_leaves_another_items() {
    // Given
    let (_data, env, sessions_dir) = an_env_with_a_deleted_item_pending();
    let github = github_source(&env);

    // When
    let answer = super::run(
        &env,
        &github,
        WorkQueueOp::DeletePlanned {
            node_id: "n2".to_string(),
        },
    )
    .expect("deleted");

    // Then
    assert_eq!(answer["deleted"], json!("n2"));
    let files = SessionWorkQueueFiles::new(&sessions_dir);
    assert!(
        files.read(OWNER, "n2", "failure.log").is_err(),
        "the deleted item's bytes are gone"
    );
    assert!(
        !sessions_dir
            .join(OWNER)
            .join("work-queue-files")
            .join("n2")
            .exists(),
        "the deleted item's directory is gone"
    );
}

#[test]
fn deleting_an_item_without_files_removes_nothing_and_still_succeeds() {
    // Given
    let (_data, env, sessions_dir) = an_env_with_a_deleted_item_pending();
    let github = github_source(&env);

    // When
    let answer = super::run(
        &env,
        &github,
        WorkQueueOp::DeletePlanned {
            node_id: "n1".to_string(),
        },
    )
    .expect("deleted");

    // Then
    assert_eq!(answer["deleted"], json!("n1"));
    assert!(
        !sessions_dir
            .join(OWNER)
            .join("work-queue-files")
            .join("n1")
            .exists(),
        "a file-less item leaves no directory behind"
    );
    // n2's files are untouched by n1's deletion.
    let files = SessionWorkQueueFiles::new(&sessions_dir);
    assert!(
        files.read(OWNER, "n2", "failure.log").is_ok(),
        "the sibling item keeps its files"
    );
}
