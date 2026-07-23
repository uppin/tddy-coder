//! The indexing task: walk → chunk → embed → store, modeled as a [`TaskBody`] so the daemon can
//! spawn it on a `TaskRegistry` and block on its terminal status before launching the agent
//! (mirrors `tddy_core::session_catalog::PopulateCatalogTask`).

use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;
use tddy_task::{TaskBody, TaskContext, TaskStatus};

use crate::chunk::chunk_worktree;
use crate::embed::Embedder;
use crate::store::{IndexedChunk, SemanticIndexStore};

/// The `kind` label recorded on the index task in the registry.
pub const SEMANTIC_INDEX_TASK_KIND: &str = "semantic_index";

/// Indexes a worktree into a per-session vector store.
pub struct SemanticIndexTask<E: Embedder> {
    /// The worktree to index.
    pub worktree_root: PathBuf,
    /// Destination vector DB (`<session_dir>/semantic-index.db`).
    pub db_path: PathBuf,
    /// The embedder used to vectorize chunks (same one the query is embedded with at search time).
    pub embedder: E,
}

impl<E: Embedder> SemanticIndexTask<E> {
    /// Walk → chunk → embed → store. Any failure bubbles up as an error for `run` to surface as
    /// `Failed`.
    async fn index(&self) -> anyhow::Result<()> {
        let chunks = chunk_worktree(&self.worktree_root).context("chunk worktree")?;

        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create session dir {}", parent.display()))?;
        }
        let store = SemanticIndexStore::open(&self.db_path).await?;

        if chunks.is_empty() {
            return Ok(());
        }

        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        let vectors = self.embedder.embed(&texts).await.context("embed chunks")?;
        anyhow::ensure!(
            vectors.len() == chunks.len(),
            "embedder returned {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        );

        let indexed: Vec<IndexedChunk> = chunks
            .into_iter()
            .zip(vectors)
            .map(|(chunk, vector)| IndexedChunk {
                source_path: chunk.source_path,
                text: chunk.text,
                vector,
            })
            .collect();
        store.insert(&indexed).await.context("persist chunks")?;
        Ok(())
    }
}

#[async_trait]
impl<E: Embedder + 'static> TaskBody for SemanticIndexTask<E> {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        if ctx.is_cancelled() {
            return TaskStatus::Cancelled;
        }
        match self.index().await {
            Ok(()) => TaskStatus::Completed { exit_code: Some(0) },
            Err(err) => TaskStatus::Failed {
                message: format!("{err:#}"),
            },
        }
    }
}
