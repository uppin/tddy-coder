//! Daemon wiring for the "Semantic index" option: a managed session with `semantic_index = true`
//! runs a blocking index into the session dir before launch (aborting the start if indexing fails)
//! and points the in-jail `SemanticSearch` tool env at that per-session index DB.
//!
//! Feature: docs/ft/coder/semantic-index.md (criteria 7-9)
//! Changeset: docs/dev/1-WIP/2026-07-22-semantic-index.md

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use tddy_daemon::semantic_index::{run_semantic_index_blocking, semantic_index_env};
use tddy_semantic_index::Embedder;
use tddy_task::TaskRegistry;
use tempfile::TempDir;
use tokio::time::timeout;

const FAKE_DIMS: usize = 8;

/// A deterministic, offline embedder — one fixed unit vector per text. No model, no network.
struct StubEmbedder;

#[async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; FAKE_DIMS];
                v[t.len() % FAKE_DIMS] = 1.0;
                v
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        FAKE_DIMS
    }
}

/// An embedder that always fails — drives the abort-on-failure path.
struct FailingEmbedder;

#[async_trait]
impl Embedder for FailingEmbedder {
    async fn embed(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        anyhow::bail!("embedding backend unavailable")
    }

    fn dimensions(&self) -> usize {
        FAKE_DIMS
    }
}

fn a_worktree_with(rel: &str, contents: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join(rel), contents).expect("write worktree file");
    dir
}

#[test]
fn semantic_index_env_points_the_tool_at_the_session_dir_database() {
    // Given — a session directory
    let session = tempfile::tempdir().expect("session dir");

    // When — deriving the SemanticSearch tool env for it
    let (key, value) = semantic_index_env(session.path());

    // Then — the tool is pointed at the per-session index DB inside the session dir
    assert_eq!(key, "TDDY_SEMANTIC_INDEX_DB", "env var name");
    assert_eq!(
        Path::new(&value),
        session.path().join("semantic-index.db"),
        "index DB must live in the session dir"
    );
}

#[tokio::test]
async fn blocking_index_writes_the_database_to_the_session_dir_and_returns_its_path() {
    // Given — a worktree to index and a separate session directory
    let worktree = a_worktree_with("lib.rs", "fn connect() { open a database connection }");
    let session = tempfile::tempdir().expect("session dir");
    let registry = TaskRegistry::new();

    // When — the blocking index step runs to completion
    let db_path = timeout(
        Duration::from_secs(5),
        run_semantic_index_blocking(
            worktree.path(),
            session.path(),
            StubEmbedder,
            &registry,
            "sess-1",
        ),
    )
    .await
    .expect("blocking index must not hang")
    .expect("indexing must succeed");

    // Then — it returns the session-dir DB path and that file now exists
    assert_eq!(
        db_path,
        session.path().join("semantic-index.db"),
        "returned path must be the session-dir index DB"
    );
    assert!(
        db_path.exists(),
        "the index must be written to the session dir at {}",
        db_path.display()
    );
}

#[tokio::test]
async fn blocking_index_aborts_with_an_error_when_indexing_fails() {
    // Given — a worktree and an embedder that always fails
    let worktree = a_worktree_with("lib.rs", "fn a() {}");
    let session = tempfile::tempdir().expect("session dir");
    let registry = TaskRegistry::new();

    // When — the blocking index step runs with the failing embedder
    let result = timeout(
        Duration::from_secs(5),
        run_semantic_index_blocking(
            worktree.path(),
            session.path(),
            FailingEmbedder,
            &registry,
            "sess-1",
        ),
    )
    .await
    .expect("blocking index must not hang");

    // Then — the failure surfaces as an error so the daemon aborts the launch (no fallback)
    assert!(
        result.is_err(),
        "a failed index must abort the start; got {result:?}"
    );
}
