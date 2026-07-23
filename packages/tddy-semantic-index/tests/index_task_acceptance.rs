//! The blocking index task: indexes a worktree into the session-dir DB, and surfaces embedding
//! failure as a terminal `Failed` status (the daemon's abort signal).
//!
//! Feature: docs/ft/coder/semantic-index.md (criteria 7-8, 12)

mod common;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use common::{FailingEmbedder, HashingEmbedder};
use tddy_semantic_index::{
    Embedder, SemanticIndexStore, SemanticIndexTask, SEMANTIC_INDEX_TASK_KIND,
};
use tddy_task::{TaskHandle, TaskRegistry, TaskStatus};
use tokio::time::timeout;

/// Block until the task reaches a terminal status (or fail loudly after 5s).
async fn await_terminal(handle: &Arc<TaskHandle>) -> TaskStatus {
    timeout(Duration::from_secs(5), async {
        let mut rx = handle.status_watch();
        let status = rx
            .wait_for(TaskStatus::is_terminal)
            .await
            .expect("status channel must stay open");
        status.clone()
    })
    .await
    .expect("index task must reach a terminal status")
}

fn a_worktree_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, contents) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("mkdir parent");
        fs::write(&path, contents).expect("write file");
    }
    dir
}

#[tokio::test]
async fn indexes_every_text_file_in_the_worktree() {
    // Given — a worktree with two topically-distinct files and a session-dir DB path
    let worktree = a_worktree_with(&[
        ("alpha.rs", "fn alpha() { open a database connection pool }"),
        (
            "beta.rs",
            "fn beta() { parse an http request and route it }",
        ),
    ]);
    let session = tempfile::tempdir().expect("session dir");
    let db_path = session.path().join("semantic-index.db");
    let registry = TaskRegistry::new();

    // When — the index task runs to completion
    let handle = registry
        .spawn(
            SemanticIndexTask {
                worktree_root: worktree.path().to_path_buf(),
                db_path: db_path.clone(),
                embedder: HashingEmbedder,
            },
            SEMANTIC_INDEX_TASK_KIND,
            "sess-index",
            vec![],
        )
        .await;
    let status = await_terminal(&handle).await;

    // Then — it completes, and both files are searchable in the resulting store
    assert_eq!(
        status,
        TaskStatus::Completed { exit_code: Some(0) },
        "indexing must complete cleanly; got {status:?}"
    );
    let store = SemanticIndexStore::open(&db_path)
        .await
        .expect("store must open");
    let query = HashingEmbedder
        .embed(&["http request".to_string()])
        .await
        .expect("embed")
        .into_iter()
        .next()
        .expect("one vector");
    let hits = store.search(&query, 2).await.expect("search must succeed");
    assert!(
        hits.iter().any(|h| h.source_path.ends_with("beta.rs")),
        "an http query must surface beta.rs; hits: {hits:?}"
    );
}

#[tokio::test]
async fn writes_the_index_database_to_the_given_session_dir_path() {
    // Given — a worktree and a nested session-dir DB path that does not yet exist
    let worktree = a_worktree_with(&[("a.rs", "fn a() { let x = 1; }")]);
    let session = tempfile::tempdir().expect("session dir");
    let db_path = session.path().join("semantic-index.db");
    let registry = TaskRegistry::new();

    // When — the index task runs to completion
    let handle = registry
        .spawn(
            SemanticIndexTask {
                worktree_root: worktree.path().to_path_buf(),
                db_path: db_path.clone(),
                embedder: HashingEmbedder,
            },
            SEMANTIC_INDEX_TASK_KIND,
            "sess-index",
            vec![],
        )
        .await;
    let status = await_terminal(&handle).await;

    // Then — the index database is written at the requested session-dir path
    assert_eq!(status, TaskStatus::Completed { exit_code: Some(0) });
    assert!(
        db_path.exists(),
        "the index must be written to the session dir at {}",
        db_path.display()
    );
}

#[tokio::test]
async fn reports_failed_when_the_embedder_fails() {
    // Given — a worktree and an embedder that always errors
    let worktree = a_worktree_with(&[("a.rs", "fn a() {}")]);
    let session = tempfile::tempdir().expect("session dir");
    let registry = TaskRegistry::new();

    // When — the index task runs with the failing embedder
    let handle = registry
        .spawn(
            SemanticIndexTask {
                worktree_root: worktree.path().to_path_buf(),
                db_path: session.path().join("semantic-index.db"),
                embedder: FailingEmbedder,
            },
            SEMANTIC_INDEX_TASK_KIND,
            "sess-index",
            vec![],
        )
        .await;
    let status = await_terminal(&handle).await;

    // Then — the failure surfaces as a terminal Failed status (the daemon's abort signal)
    assert!(
        matches!(status, TaskStatus::Failed { .. }),
        "an embedding failure must surface as Failed, not Completed; got {status:?}"
    );
}
