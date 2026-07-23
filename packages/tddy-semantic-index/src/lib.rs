//! `tddy-semantic-index` — per-session code embedding index and semantic search.
//!
//! A managed-codebase session with "Semantic index" enabled indexes its worktree before the agent
//! starts: files are chunked ([`chunk`]), each chunk is embedded by an [`Embedder`] ([`embed`]), and
//! the vectors are persisted to a per-session `sqlite-vec` store ([`store`]) at
//! `<session_dir>/semantic-index.db`. Indexing runs as a [`SemanticIndexTask`] ([`index_task`]) so
//! the daemon can spawn it on a `tddy_task::TaskRegistry` and block on its terminal status before
//! launch. The `SemanticSearch` tool then queries the store via a [`SemanticSearchEngine`]
//! ([`search`]).
//!
//! The embedding model is injected behind the [`Embedder`] trait so the pipeline is deterministic
//! and offline under test; the production default is a local bundled model.
//!
//! Feature: `docs/ft/coder/semantic-index.md`.

pub mod chunk;
pub mod embed;
pub mod index_task;
pub mod local_model;
pub mod search;
pub mod store;

pub use chunk::{chunk_worktree, Chunk};
pub use embed::Embedder;
pub use index_task::{SemanticIndexTask, SEMANTIC_INDEX_TASK_KIND};
pub use local_model::{embedding_model_dir, production_embedder};
pub use search::SemanticSearchEngine;
pub use store::{IndexedChunk, SearchHit, SemanticIndexStore};

#[cfg(feature = "local-model")]
pub use local_model::LocalEmbedder;
