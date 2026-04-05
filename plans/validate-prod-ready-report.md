# Validate prod-ready — Semantic session search

**Scope:** `session_semantic_search.rs`, `connection_service.rs` (`SearchSessions` RPC), `error.rs` (`SessionSearchIndex`), `SessionSearchInput.tsx`, `connection.proto` (`SearchSessions`), and call sites of `index_session_for_search`.  
**Date:** 2026-04-05

---

## Executive summary

The stack implements **local** semantic search over a SQLite index (`session_search_index.sqlite3` under the Tddy sessions data root), deterministic on-device “hash-trick” embeddings (no network), and a gated **`SearchSessions`** RPC that scopes results by authenticated user → OS user → `sessions_base`. Core library behavior is documented for **migrations and recovery** (delete bad DB; re-index from each session’s `changeset.yaml`).

**Blocking operational gap:** `index_session_for_search` is **not invoked from any production code path**—only from tests and the library itself. Unless another process manually builds the index, **`SearchSessions` will usually return no hits** (missing DB) or **stale** data if the file was populated out of band. That matches a typical “evaluate” finding: **the index is never populated in normal prod flows.**

Secondary concerns: search runs **synchronously inside an `async` RPC** (no `spawn_blocking`), **full-table scan + in-memory sort** with no server-side cap on result size, and **no index row removal** when sessions are deleted.

**Overall rating: partial — not production-ready** for end users expecting search to work out of the box. The RPC/UI/embeddings layer is a reasonable **beta** of the retrieval path *if* indexing is wired and ops expectations are clear; until then it is **feature-incomplete**, not merely “rough.”

---

## `index_session_for_search` — production vs tests

| Location | Role |
|----------|------|
| `packages/tddy-core/src/session_semantic_search.rs` | Definition (`pub fn index_session_for_search`) |
| `packages/tddy-integration-tests/tests/semantic_session_search_acceptance.rs` | Calls in acceptance tests |
| `packages/tddy-daemon/tests/semantic_search_sessions_rpc.rs` | Calls to seed index before RPC assertions |

**Production crates (daemon, coder, tools, etc.):** no references that call `index_session_for_search`. The daemon only calls `search_sessions_semantic`.

---

## Error handling

| Layer | Behavior |
|-------|----------|
| **tddy-core** | SQLite and validation issues map to `WorkflowError::SessionSearchIndex(String)` (`error.rs`). `read_changeset` during indexing can surface `ChangesetMissing` / `ChangesetInvalid` etc. from the changeset module. |
| **Missing index file** | `search_sessions_semantic` returns **`Ok(vec![])`** — not an error (see `session_semantic_search.rs`). |
| **Empty / whitespace query** | `Ok(vec![])` after trim. |
| **Per-row embedding issues** | Wrong dimension: row **skipped** with `log::warn!`, search continues. Model id mismatch: **warn**, row still scored. |
| **Daemon RPC** | Non-OK paths from `search_sessions_semantic` map to **`Status::internal`** with `format!("{}", e)` — generic internal error for clients; no structured code for “corrupt DB” vs “permission denied.” |

**Assessment:** Acceptable for a local tool; **differentiation of failure modes** (corrupt DB vs I/O) is weak at the RPC boundary. Empty results are ambiguous (no index vs weak query vs below score floor).

---

## Logging (levels and sensitivity)

| Event | Level | Notes |
|-------|-------|--------|
| Schema `user_version` | `debug` | Appropriate. |
| Migration to schema v1 | `info` | Reasonable once per process/DB. |
| `index_session_for_search` start | `info` | Logs **session_id** and **full `session_dir` path** — useful for ops, noisy if indexing were high-frequency. |
| Index doc field lengths | `debug` | Appropriate. |
| Search: query length, missing DB, score floor | `debug` | Appropriate. |
| Search: hit count | `info` | Can be chatty on every keystroke-driven RPC unless the client debounces well. |
| Model / dim mismatch | `warn` | Appropriate. |
| **Daemon `SearchSessions`** | `debug` | Logs **`query_len` and `github_user`** — user identity in logs may be undesirable in strict privacy deployments; query content is not logged here. |

**Assessment:** Levels are mostly sane; **`info` on every index call** and **`info` hit counts on every successful search** may be heavy under load. Consider **`debug`** for per-request search summaries if log volume matters.

---

## Configuration and environment

- **No dedicated env vars** for semantic search: index path is **`{sessions_base}/session_search_index.sqlite3`** (`session_search_index_path`), where `sessions_base` comes from the same resolver used for other session RPCs (`connection_service.rs`).
- **Embedding model id and dimension** are **compile-time constants** (`SESSION_SEARCH_EMBEDDING_MODEL_ID`, `SESSION_SEARCH_EMBEDDING_DIM`, `SESSION_SEARCH_INDEX_SCHEMA_VERSION`) — good for reproducibility; changing them requires code + migration strategy.

---

## Security

| Topic | Assessment |
|-------|------------|
| **Data locality** | Index and session dirs live under the authenticated user’s Tddy data tree; **no cloud embedding calls** in v1 (hash-trick only). |
| **Authorization** | Same pattern as `ListSessions`: `session_token` → GitHub user → OS user → `sessions_base`. **No path traversal via search query** — query is text for embedding/scoring only. |
| **Exposure** | Hits include `initial_prompt`, worktree/branch labels — **sensitive local dev content**, but consistent with “search my sessions” and same trust domain as reading `changeset.yaml` on disk. |
| **Cross-tenant** | Relies on correct `sessions_base` resolution; if that is wrong, impact is broader than search — **inherited from daemon session path model**. |

---

## Performance

| Issue | Detail |
|-------|--------|
| **Full table scan** | `SELECT session_id, ... FROM session_search_index` — every row loaded and scored. **O(n)** sessions per query. |
| **No SQL `LIMIT`** | All rows collected, then sorted in memory; **unbounded** result vector size before client receives (filtering is score-based + `MIN_BEST_SCORE` floor, not top-k). |
| **Async handler** | `search_sessions` **does not** use `spawn_blocking` (contrast `list_sessions`). SQLite + CPU work can **block the Tokio worker** during the call. |
| **SQLite** | `SQLITE_OPEN_FULL_MUTEX` — safe for concurrent access from multiple threads; still **one writer** typical for SQLite. |

**Assessment:** Acceptable for **small to moderate** session counts on a workstation; **risk** grows with index size and concurrent searches without a blocking pool or limits.

---

## Operational gaps

1. **Index never populated in prod** — `index_session_for_search` has **no production caller**; search is effectively **empty or manually maintained**.
2. **No incremental updates** when `changeset.yaml` changes during a workflow (no hook from tddy-coder/daemon).
3. **DeleteSession** does not remove rows from `session_search_index` — **orphan rows** if indexing is ever automated.
4. **No documented operator command** in-repo (beyond module comments) to **backfill** or **rebuild** the index in production.

---

## Migrations and corruption recovery (as coded)

- **Module docs** (`session_semantic_search.rs`): incompatible or corrupt DB files **may be deleted**; sessions **re-index** from each session dir’s `changeset.yaml` via `index_session_for_search`.
- **`ensure_schema`:** If `user_version < SESSION_SEARCH_INDEX_SCHEMA_VERSION`, runs `CREATE TABLE IF NOT EXISTS` and bumps `user_version`. **First-time create** is covered; **future schema upgrades** will need explicit migration steps (current logic is minimal).
- **Partial corruption:** Bad embedding length → row skipped; **bad DB file** may still cause `open`/`query` errors → **internal** to clients.

---

## Web UI (`SessionSearchInput.tsx`)

- **Debounced** `onSearchQuery` (default 300 ms) to avoid RPC spam — good client-side hygiene.
- Component does **not** implement RPC itself; **parent must wire** `SearchSessions` — no extra security surface in this file.
- **No loading/error UI** in the component — product behavior depends on parent.

---

## Proto (`connection.proto`)

- **`SearchSessionsRequest`:** `session_token`, `query`.
- **`SearchSessionHit`:** `session_id`, `initial_prompt`, `relevance_score`, `worktree_label`, `branch_label`.
- **No pagination or `max_results`** — large hit lists possible if many sessions score above the internal floor.

---

## Overall production readiness

| Rating | **Partial — not production-ready** (end-to-end feature). |
|--------|----------------------------------------------------------|
| **Rationale** | Retrieval and auth wiring exist and are test-covered, but **without a production indexing lifecycle** the feature does not deliver value. Additional gaps: **blocking RPC**, **unbounded scans/results**, **no deletion sync**, **limited RPC error semantics**. |
| **“Good for beta”?** | **Only** if beta explicitly means “try RPC + UI with manually seeded index” or indexing is added before release. Otherwise treat as **incomplete**. |

---

## Suggested priorities before “prod ready”

1. **Wire `index_session_for_search`** (or a batch backfill) from real lifecycle events (session create/update, or periodic reconciliation), and **remove index rows on session delete**.
2. Run search work in **`spawn_blocking`** (or dedicated pool) with a **timeout** aligned with other daemon blocking ops.
3. Consider **top-k / cap** on returned hits and/or score threshold configurability for large deployments.
4. Tighten **logging** (reduce `info` noise; avoid user identifiers in logs if policy requires).
