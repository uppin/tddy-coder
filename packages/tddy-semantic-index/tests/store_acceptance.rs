//! The per-session `sqlite-vec` store: persists chunk vectors and returns nearest matches.
//!
//! Feature: docs/ft/coder/semantic-index.md (criterion 12-13)

mod common;

use std::time::Duration;

use common::{an_indexed_chunk, HashingEmbedder};
use tddy_semantic_index::{Embedder, IndexedChunk, SemanticIndexStore};
use tokio::time::timeout;

async fn a_store_seeded_with(chunks: Vec<IndexedChunk>) -> (tempfile::TempDir, SemanticIndexStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SemanticIndexStore::open(&dir.path().join("semantic-index.db"))
        .await
        .expect("store must open");
    store.insert(&chunks).await.expect("insert must succeed");
    (dir, store)
}

async fn embed_query(embedder: &HashingEmbedder, text: &str) -> Vec<f32> {
    embedder
        .embed(&[text.to_string()])
        .await
        .expect("embed")
        .into_iter()
        .next()
        .expect("one vector")
}

#[tokio::test]
async fn returns_the_chunk_nearest_to_the_query_vector_first() {
    // Given — three chunks about distinct topics
    let embedder = HashingEmbedder;
    let (_dir, store) = a_store_seeded_with(vec![
        an_indexed_chunk(&embedder, "db.rs", "open sqlite database connection pool").await,
        an_indexed_chunk(
            &embedder,
            "http.rs",
            "parse http request headers and routes",
        )
        .await,
        an_indexed_chunk(
            &embedder,
            "math.rs",
            "compute vector cosine similarity score",
        )
        .await,
    ])
    .await;

    // When — searching for a database-connection query
    let query = embed_query(&embedder, "database connection").await;
    let hits = timeout(Duration::from_secs(5), store.search(&query, 3))
        .await
        .expect("search must not hang")
        .expect("search must succeed");

    // Then — the database chunk ranks first
    assert_eq!(
        hits.first().map(|h| h.source_path.as_str()),
        Some("db.rs"),
        "nearest hit must be db.rs; hits: {hits:?}"
    );
}

#[tokio::test]
async fn limits_the_results_to_the_requested_k() {
    // Given — three indexed chunks that all share a token
    let embedder = HashingEmbedder;
    let (_dir, store) = a_store_seeded_with(vec![
        an_indexed_chunk(&embedder, "a.rs", "alpha token one").await,
        an_indexed_chunk(&embedder, "b.rs", "beta token two").await,
        an_indexed_chunk(&embedder, "c.rs", "gamma token three").await,
    ])
    .await;

    // When — asking for the two nearest
    let query = embed_query(&embedder, "token").await;
    let hits = store.search(&query, 2).await.expect("search must succeed");

    // Then — exactly two results come back
    assert_eq!(
        hits.len(),
        2,
        "K=2 must cap the result count; got {}",
        hits.len()
    );
}

#[tokio::test]
async fn returns_the_source_path_and_text_of_each_hit() {
    // Given — a single indexed chunk
    let embedder = HashingEmbedder;
    let (_dir, store) = a_store_seeded_with(vec![
        an_indexed_chunk(&embedder, "only.rs", "unique indexable content here").await,
    ])
    .await;

    // When — searching for its content
    let query = embed_query(&embedder, "unique indexable content here").await;
    let hits = store.search(&query, 1).await.expect("search must succeed");

    // Then — the hit carries the chunk's source path and text
    let hit = hits.first().expect("one hit");
    assert_eq!(hit.source_path, "only.rs", "hit source path");
    assert_eq!(hit.text, "unique indexable content here", "hit text");
}
