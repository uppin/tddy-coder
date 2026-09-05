# 2026-07-23 — Semantic index orchestration

**Type:** Feature

new `semantic_index` module (`semantic_index_db_path`, `semantic_index_env` → `TDDY_SEMANTIC_INDEX_DB`, `run_semantic_index_blocking` → blocks on a `SemanticIndexTask`; `Err` aborts the start). Wired into all four managed start paths (claude+cursor × sandboxed+non-sandboxed): when `req.semantic_index`, runs `production_embedder(data_dir)?` + the blocking index before launch and injects the env pair. Non-default `local-model` passthrough feature. Feature [semantic-index.md](../../../../docs/ft/coder/semantic-index.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
