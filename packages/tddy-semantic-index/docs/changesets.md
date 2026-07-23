# Changesets Applied

Wrapped changeset history for tddy-semantic-index.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-23** [Feature] **New crate: per-session code embedding index + semantic search** — `chunk_worktree` (walk + whole-file text chunks), `SemanticIndexStore` (`rusqlite` + `sqlite-vec` `vec0` KNN at `<session_dir>/semantic-index.db`), blocking `SemanticIndexTask` (`tddy-task`; walk→chunk→embed→store, `Completed`/`Failed`), `SemanticSearchEngine`, and an injected `Embedder` trait. Production `LocalEmbedder` (candle `all-MiniLM-L6-v2`, 384-dim, mean-pool + L2-normalize, hf-hub fetch-cache under the data dir) behind the non-default `local-model` feature; `production_embedder(data_dir)` errors without it. Tests: store 3, index-task 3, search 1, embedder-factory 2, + a `TDDY_SEMANTIC_MODEL_TEST`-gated inference smoke test. Feature [semantic-index.md](../../../docs/ft/coder/semantic-index.md). Cross-package [docs/dev/changesets.md](../../../docs/dev/changesets.md). (tddy-semantic-index)
