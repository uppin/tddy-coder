//! Online smoke test for the production candle embedder. Fetching the model requires network and a
//! ~90MB download, so this is opt-in: it runs only under the `local-model` feature AND when
//! `TDDY_SEMANTIC_MODEL_TEST=1` is set (mirroring the repo's env-gated testkit convention, e.g.
//! `LIVEKIT_TESTKIT_WS_URL`). Default and CI runs never touch the network.
//!
//! Feature: docs/ft/coder/semantic-index.md (criterion 14)
#![cfg(feature = "local-model")]

use tddy_semantic_index::production_embedder;

/// Cosine similarity of two equal-length vectors (both are unit-norm here, so this is a dot product).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[tokio::test]
async fn embeds_text_into_unit_vectors_that_rank_related_meaning_higher() {
    // Given — the opt-in guard and the production embedder over a fresh cache dir
    if std::env::var("TDDY_SEMANTIC_MODEL_TEST").as_deref() != Ok("1") {
        return;
    }
    let data = tempfile::tempdir().expect("tempdir");
    let embedder = production_embedder(data.path()).expect("build production embedder");

    let query = "how do I read a file from disk".to_string();
    let related = "opening and reading the contents of a file".to_string();
    let unrelated = "the best recipe for chocolate cake".to_string();

    // When — embedding the query and two candidates in one batch
    let vectors = embedder
        .embed(&[query, related, unrelated])
        .await
        .expect("embed batch");

    // Then — 384-dim unit vectors, and the related sentence is nearer than the unrelated one
    assert_eq!(embedder.dimensions(), 384);
    assert_eq!(vectors.len(), 3);
    for vector in &vectors {
        assert_eq!(vector.len(), 384);
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "expected unit-norm, got {norm}");
    }
    let related_score = cosine(&vectors[0], &vectors[1]);
    let unrelated_score = cosine(&vectors[0], &vectors[2]);
    assert!(
        related_score > unrelated_score,
        "related pair ({related_score}) should score higher than unrelated pair ({unrelated_score})"
    );
}
