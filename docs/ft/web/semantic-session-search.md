# Semantic session search

## Summary

Workflow sessions are searchable by natural-language queries against text derived from each session’s **`changeset.yaml`**: **`initial_prompt`**, merged worktree label (**`worktree`** or **`worktree_suggestion`**), and merged branch label (**`branch`** or **`branch_suggestion`**). A **local SQLite** database stores rows keyed by session id with **deterministic on-device embeddings** (hashing-trick vectors, pinned model id **`tddy-hash-trick-v1-dim256`**, dimension **256**). The **Connect-RPC** method **`SearchSessions`** on **`ConnectionService`** returns ranked hits with **`session_id`**, **`initial_prompt`**, **`worktree_label`**, **`branch_label`**, and **`relevance_score`**. The **tddy-web** component **`SessionSearchInput`** debounces user input before invoking a search callback (component tests cover the debounce contract).

## Index location

- **File**: **`session_search_index.sqlite3`** at the **Tddy data directory root** (the same root that contains the **`sessions/`** subtree), i.e. **`{data_root}/session_search_index.sqlite3`** where **`data_root`** resolves like other session tooling (**`TDDY_SESSIONS_DIR`**, in-process override, or default home-based path per **`tddy_core::output::tddy_data_dir_path`**).
- **Schema**: **`PRAGMA user_version`** tracks migrations; table **`session_search_index`** holds **`session_id`**, text fields, **`embedding`** blob, **`embedding_dim`**, **`embedding_model_id`**, **`document_text`**, **`updated_at`**.

## Retrieval behavior

- **Empty or whitespace-only queries** yield **no hits** at the RPC layer (after trim).
- **Search** ranks stored sessions by a **relevance score** combining embedding similarity (cosine on normalized vectors) with lexical signals (substring and token overlap). A **minimum best-score floor** suppresses noise when nothing matches strongly.
- **Privacy**: Embeddings and indexed text stay **on disk on the user’s machine**; there is **no network** embedding call in the default implementation.

## Index maintenance

- **`tddy_core::session_semantic_search::index_session_for_search`** reads **`changeset.yaml`** from a session directory and **upserts** one row per session id.
- **Recovery**: If the SQLite file is corrupt or incompatible, operators may **delete** **`session_search_index.sqlite3`** and **rebuild** by re-running indexing for each session directory (there is no separate admin CLI in-tree).

## Related documentation

- **Daemon RPC**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md) — **`SearchSessions`**.
- **Web shell**: [web-terminal.md](web-terminal.md) — connection screen context.
- **Core implementation**: `packages/tddy-core/src/session_semantic_search.rs`.
