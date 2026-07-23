//! The production embedder factory + its model-cache location. The candle model inference itself is
//! feature-gated (`local-model`) and network-dependent, so these offline tests pin the two
//! build-independent contracts: where weights are cached, and how a build without the model behaves.
//!
//! Feature: docs/ft/coder/semantic-index.md (criterion 14)

use tddy_semantic_index::embedding_model_dir;
// `production_embedder` is only referenced by the no-feature test below; gating the import keeps
// `--all-targets --features local-model` free of an unused-import warning.
#[cfg(not(feature = "local-model"))]
use tddy_semantic_index::production_embedder;

#[test]
fn caches_the_model_under_the_given_data_dir() {
    // Given — a tddy data directory
    let data = tempfile::tempdir().expect("tempdir");

    // When — resolving where model weights are cached
    let model_dir = embedding_model_dir(data.path());

    // Then — the cache lives under the provided data dir (fetched weights are reused across sessions)
    assert!(
        model_dir.starts_with(data.path()),
        "model cache must live under the tddy data dir; got {}",
        model_dir.display()
    );
}

#[cfg(not(feature = "local-model"))]
#[test]
fn errors_when_built_without_the_local_model_feature() {
    // Given — a build without the `local-model` feature and a data dir
    let data = tempfile::tempdir().expect("tempdir");

    // When — asking for the production embedder
    let result = production_embedder(data.path());

    // Then — it is an error (no embedder can be produced in this build), not a silent stub
    assert!(
        result.is_err(),
        "a build without the local-model feature must not yield an embedder"
    );
}
