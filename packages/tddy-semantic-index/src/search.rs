//! The query side: embed a query with the same [`Embedder`] used at index time, then KNN-search the
//! [`SemanticIndexStore`]. Backs the `SemanticSearch` tool.

use crate::embed::Embedder;
use crate::store::{SearchHit, SemanticIndexStore};

/// Embeds queries and searches a per-session vector store.
pub struct SemanticSearchEngine<E: Embedder> {
    store: SemanticIndexStore,
    embedder: E,
}

impl<E: Embedder> SemanticSearchEngine<E> {
    /// Build an engine over an opened store and the embedder used at index time.
    pub fn new(store: SemanticIndexStore, embedder: E) -> Self {
        Self { store, embedder }
    }

    /// Embed `query` and return the top-`k` nearest chunks, nearest first.
    pub async fn search(&self, query: &str, k: usize) -> anyhow::Result<Vec<SearchHit>> {
        let mut vectors = self.embedder.embed(&[query.to_string()]).await?;
        let vector = vectors
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedder returned no vector for the query"))?;
        self.store.search(&vector, k).await
    }
}
