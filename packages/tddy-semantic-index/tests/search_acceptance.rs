//! The search engine: embeds a query with the index-time embedder and returns the nearest chunk.
//!
//! Feature: docs/ft/coder/semantic-index.md (criterion 13)

mod common;

use common::{an_indexed_chunk, HashingEmbedder};
use tddy_semantic_index::{SemanticIndexStore, SemanticSearchEngine};

#[tokio::test]
async fn embeds_the_query_and_returns_the_nearest_chunk() {
    // Given — a store seeded with two topically-distinct chunks
    let embedder = HashingEmbedder;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SemanticIndexStore::open(&dir.path().join("semantic-index.db"))
        .await
        .expect("store must open");
    store
        .insert(&[
            an_indexed_chunk(
                &embedder,
                "auth.rs",
                "validate user password and issue session token",
            )
            .await,
            an_indexed_chunk(
                &embedder,
                "geometry.rs",
                "compute the area of a triangle from base and height",
            )
            .await,
        ])
        .await
        .expect("insert must succeed");
    let engine = SemanticSearchEngine::new(store, HashingEmbedder);

    // When — searching by meaning for the auth topic
    let hits = engine
        .search("user password token", 2)
        .await
        .expect("search must succeed");

    // Then — the auth chunk is nearest
    assert_eq!(
        hits.first().map(|h| h.source_path.as_str()),
        Some("auth.rs"),
        "auth query must rank auth.rs first; hits: {hits:?}"
    );
}
