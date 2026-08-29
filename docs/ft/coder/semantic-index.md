# Semantic Index (managed-codebase code index + `SemanticSearch`)

**Product area:** Coder
**Status:** Draft
**Updated:** 2026-07-22

## Summary

A managed-codebase session gains an optional **"Semantic index"** control. When enabled, the daemon
**indexes the session's worktree into a per-session vector database before the agent starts** — the
launch blocks until indexing is terminal — and makes the **`SemanticSearch`** tool available to the
agent, backed by that index. When the option is off, `SemanticSearch` is **removed from the
session's tool set** entirely (the agent cannot call it), and no indexing runs.

The index is a real embedding/vector pipeline: worktree files are chunked, each chunk is embedded by
a **local embedding model**, and the vectors are stored in a per-session SQLite database using the
**`sqlite-vec`** extension. `SemanticSearch` embeds the query with the same model and returns the
nearest chunks by vector similarity.

**The index is built where the worktree is.** For an ordinary co-located session that is the daemon
the session was started on. For a [split placement](../daemon/remote-managed-worktree.md) — agent on
one host, codebase on another — it is the **codebase host**, which builds the index for its
`workspace` session against the checkout that exists there. Requesting an index is never refused over
the placement; there is exactly one worktree that counts, and the index is built beside it.

> ⚠️ **Not yet answerable.** `SemanticSearch` cannot serve a query on **any** session shape yet:
> `tool_semantic_search` (`packages/tddy-tool-engine/src/lib.rs`) returns *"index query not yet
> wired"* because the query-side embedder is not wired into the tool engine. What is delivered today
> is the index being **built**, on the correct host; the query path below is still the target state.

> **Terminology.** This is a third, orthogonal axis of "managed codebase". It composes with the
> [workflow dimension](managed-codebase-workflow.md) (recipe-driven state machine) and the
> [remote-filesystem dimension](managed-codebase-subagents.md) (repo reached only via exec tools).
> "Semantic index" concerns only whether the session has a searchable code index.

## Background

`SemanticSearch` exists in the tool catalog today (`tddy-tool-engine`), but its implementation is a
ripgrep-backed lexical fallback — there is no index, no embeddings, and no vector store anywhere in
the workspace. The only persistence precedent is the per-session SQLite **session catalog**
(`tddy-core::session_catalog`), which is populated by a [`tddy_task`] on worktree-open and whose
first read **blocks until the populate task is terminal**. This feature reuses that exact
task-and-block pattern for indexing, and reuses the existing per-session tool-gating path (a tool
named in the effective "replaced" set is dropped from the agent's allowlist and hard-disabled).

## User story

As a developer starting a managed-codebase session on a large repo, I want the agent to be able to
find code by meaning rather than exact text, so I enable "Semantic index"; the session takes a
moment to index before the agent starts, and from then on the agent's `SemanticSearch` tool returns
semantically-relevant code. When I leave it off, the agent has no `SemanticSearch` tool at all and
starts immediately.

## Architecture

```
Web CreateSessionPane (claude-cli | cursor-cli)
  [x] Managed codebase
      Recipe:    [ tdd ▾ ]
      Subagents: [x] fastcontext
      [x] Semantic index          ──────►  StartSessionRequest.semantic_index = true
        │
        ▼
  daemon start_(sandboxed_)claude_cli_session | …cursor_cli_session
        │  (split placement ⇒ forwarded to the codebase host, which runs the same task
        │   against its own worktree for the session's `workspace` half)
        │  (managed_codebase && semantic_index)
        │  ── BLOCKING ── SemanticIndexTask (tddy_task) on the worktree:
        │        walk → chunk → embed (local model) → store vectors in
        │        <session_dir>/semantic-index.db  (sqlite-vec)
        │        └─ task Failed  ⇒  abort StartSession with an error (no launch, no fallback)
        │        └─ task Completed ⇒ continue
        │  tool gating: SemanticSearch stays in the tool set (index present)
        │  inject env: TDDY_SEMANTIC_INDEX_DB=<session_dir>/semantic-index.db
        ▼
  Agent runs → SemanticSearch(query)
        │  embed query (same model) → sqlite-vec KNN over the session index
        ▼
  ranked code chunks

  (semantic_index = false ⇒ no index task, SemanticSearch added to the session's
   "replaced" set ⇒ dropped from --allowedTools and hard-disabled via --disallowedTools)
```

## Acceptance criteria

### Web (CreateSessionPane)

1. When **Managed codebase** is enabled (for `session_type` `claude-cli` **or** `cursor-cli`), a
   **"Semantic index"** checkbox (`create-session-semantic-index-toggle`) is shown inside the
   expanded managed-codebase section, alongside the recipe picker and subagent list.
2. The Semantic index checkbox is **absent** while Managed codebase is disabled, and **absent** for
   the `"tool"` session type. Choosing a codebase host does **not** withdraw it: it stays on offer for
   a split placement, and the chosen value is submitted.
3. Semantic index defaults to **off**; a managed session created without touching it sends
   `semantic_index = false`.
4. Enabling Managed codebase + Semantic index and submitting sends `semantic_index = true` on
   `StartSessionRequest` (for both `claude-cli` and `cursor-cli`).
5. Disabling Managed codebase after Semantic index was checked sends `semantic_index = false` — the
   value never leaks from a hidden control (mirrors the specialized-subagents leak-guard).

### Proto

6. `StartSessionRequest` carries a `bool semantic_index` field; regenerated for Rust and TS.

### Daemon (StartSession → claude-cli | cursor-cli)

7. A managed session with `semantic_index = true` runs a **blocking** `SemanticIndexTask` over the
   worktree before launching the agent; the agent process is not spawned until the task reaches a
   terminal status.
8. If the index task **fails**, `StartSession` returns an error and the agent is **never launched** —
   no silent fallback to an unindexed session.
9. A session with `semantic_index = true` launches with `SemanticSearch` in its tool set and an
   environment that points the tool at the session's index DB.
10. A session with `semantic_index = false` (or a non-managed session) runs **no** index task and
    launches with `SemanticSearch` **removed** from its tool set: absent from `--allowedTools` and
    present in `--disallowedTools` (both native and `mcp__tddy-tools__` forms), so the agent cannot
    call it. Behavior for all other tools is unchanged.
11. Applies to **both** sandboxed and non-sandboxed **claude-cli** and **cursor-cli** managed
    sessions.
12. A **split** session started with `semantic_index = true` is not refused over the field: the
    request is forwarded to the codebase host, which builds the index against the worktree it holds.
    Nothing is indexed on the agent host, which has no checkout to index.

### Semantic index engine

13. Indexing walks the worktree, chunks text files, embeds each chunk with the configured local
    embedding model, and stores `(chunk text, source path, vector)` rows in a per-session
    `sqlite-vec` database at `<session_dir>/semantic-index.db`.
14. `SemanticSearch(query)` embeds the query with the same model and returns the top-K chunks ranked
    by vector similarity (nearest first), each with its source path and text.
15. The embedding model is injected behind an `Embedder` trait so the pipeline is deterministic and
    model-free under test; the production default is the local bundled model.
16. When `SemanticSearch` is invoked without an available index, it returns a clear error — it does
    **not** fall back to a lexical/ripgrep search.

## Non-goals (v1)

- **Progress reporting** for indexing (the task blocks silently; no live progress to the UI). A
  follow-up may stream `SemanticIndexTask` progress like other `tddy_task`s.
- **Incremental / re-indexing** on file change — the index is built once at session start.
- Making Semantic index available to the `"tool"` session type.
- Sharing an index across sessions or persisting it beyond the session directory.
- Tuning chunking strategy, embedding model choice, or similarity metric beyond a working default.

## Related

- [Managed codebase (workflow)](managed-codebase-workflow.md) — the checkbox and section this feature extends.
- [Managed codebase (remote filesystem)](managed-codebase-subagents.md) — the other "managed" axis.
- [Session catalog](session-catalog.md) — the per-session SQLite + blocking-populate `tddy_task` precedent this reuses.
