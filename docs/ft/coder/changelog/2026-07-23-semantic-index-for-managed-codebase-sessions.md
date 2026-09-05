# 2026-07-23 — Semantic index for managed-codebase sessions

- Managed-codebase sessions (Claude/Cursor CLI) gain an optional **Semantic index** toggle: when on, the daemon indexes the worktree into a per-session `sqlite-vec` DB at `<session_dir>/semantic-index.db` before the agent starts, blocking launch and aborting the start if indexing fails (no fallback) ([semantic-index.md](../semantic-index.md)).
- Indexing embeds file chunks with a local candle model (`all-MiniLM-L6-v2`, behind the non-default `local-model` build feature; weights fetched & cached under the tddy data dir); the embedder is injected behind an `Embedder` trait so the pipeline is deterministic and offline under test.
- `SemanticSearch` is available only when a session is indexed: for sandboxed Claude it is dropped from the allow-list and hard-disabled otherwise; Cursor and non-sandboxed Claude have no allow-list surface, so it instead errors at call time when no index exists. Its former ripgrep fallback is removed.
- New crate `tddy-semantic-index` (chunker, `sqlite-vec` store, blocking index task, search engine, embedder). Web adds the toggle + `semantic_index` on `StartSessionRequest`.
