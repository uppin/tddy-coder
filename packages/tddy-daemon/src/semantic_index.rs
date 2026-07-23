//! Per-session "Semantic index" orchestration for managed-codebase sessions.
//!
//! When a managed `claude-cli`/`cursor-cli` session is started with `semantic_index = true`, the
//! daemon:
//! 1. runs a **blocking** [`tddy_semantic_index::SemanticIndexTask`] over the worktree, writing the
//!    vector DB to `<session_dir>/semantic-index.db`, and **aborts the start** if indexing fails;
//! 2. injects `TDDY_SEMANTIC_INDEX_DB=<that path>` into the session env so the in-jail
//!    `SemanticSearch` tool resolves against the per-session index.
//!
//! Tool availability itself is gated by
//! [`tddy_sandbox_recipes::effective_replaced_tools`] (SemanticSearch joins the "replaced" set when
//! indexing is off). See docs/ft/coder/semantic-index.md.

use std::path::{Path, PathBuf};

/// Environment variable the in-jail `SemanticSearch` tool reads to locate the session's index DB.
const SEMANTIC_INDEX_DB_ENV: &str = "TDDY_SEMANTIC_INDEX_DB";

/// The per-session vector DB path: `<session_dir>/semantic-index.db`.
pub fn semantic_index_db_path(session_dir: &Path) -> PathBuf {
    session_dir.join("semantic-index.db")
}

/// The `(key, value)` env pair that points the `SemanticSearch` tool at the session's index DB.
pub fn semantic_index_env(session_dir: &Path) -> (String, String) {
    let db_path = semantic_index_db_path(session_dir);
    (
        SEMANTIC_INDEX_DB_ENV.to_string(),
        db_path.to_string_lossy().into_owned(),
    )
}

/// Runs the semantic index over `worktree_root`, writing the vector DB to the session dir, and
/// blocks until the indexing task reaches a terminal status.
///
/// Returns the DB path on success, or an error message if indexing failed or was cancelled — the
/// caller aborts the session start on `Err` (no fallback).
pub async fn run_semantic_index_blocking<E: tddy_semantic_index::Embedder + 'static>(
    worktree_root: &Path,
    session_dir: &Path,
    embedder: E,
    registry: &tddy_task::TaskRegistry,
    session_id: &str,
) -> Result<PathBuf, String> {
    let db_path = semantic_index_db_path(session_dir);

    let task = tddy_semantic_index::SemanticIndexTask {
        worktree_root: worktree_root.to_path_buf(),
        db_path: db_path.clone(),
        embedder,
    };

    let handle = registry
        .spawn(
            task,
            tddy_semantic_index::SEMANTIC_INDEX_TASK_KIND,
            session_id,
            vec![],
        )
        .await;

    let mut status = handle.status_watch();
    let terminal = status
        .wait_for(tddy_task::TaskStatus::is_terminal)
        .await
        .map_err(|e| format!("semantic index task status channel closed: {e}"))?;

    match &*terminal {
        tddy_task::TaskStatus::Completed { .. } => Ok(db_path),
        tddy_task::TaskStatus::Failed { message } => Err(message.clone()),
        tddy_task::TaskStatus::Cancelled => Err("semantic index task was cancelled".into()),
        // `wait_for` only returns once the predicate holds, i.e. on a terminal status.
        non_terminal => Err(format!(
            "semantic index task ended in non-terminal status: {non_terminal:?}"
        )),
    }
}
