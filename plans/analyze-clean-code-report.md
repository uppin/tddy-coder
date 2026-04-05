# Semantic session search — code quality analysis

Scope: `session_semantic_search.rs`, `connection_service.rs` (`search_sessions`), `SessionSearchInput.tsx`, and related tests (`session_semantic_search_unit.rs`, `semantic_search_sessions_rpc.rs`, `semantic_session_search_acceptance.rs`, `SessionSearchInput.cy.tsx`).

---

## Naming consistency

**Strengths**

- Rust module `session_semantic_search` matches the feature; public APIs (`index_session_for_search`, `search_sessions_semantic`, `SessionSearchHit`) read clearly.
- RPC surface uses `SearchSessions` / `SearchSessionsRequest` / `SearchSessionsResponse` consistently with `connection.proto` and generated TS.

**Inconsistencies / watchouts**

- **Proto vs Rust hit type:** generated proto uses `SearchSessionHit` (verb-first) while core uses `SessionSearchHit` (noun-first). The daemon maps field-by-field; both are fine in isolation but the asymmetry can confuse grep and onboarding. A one-line comment at the mapping site in `connection_service.rs` would document the intentional rename.
- **“Semantic” in names:** the RPC is generic `SearchSessions`; only the core path (`session_semantic_search`, `search_sessions_semantic`) signals the hashing-trick / semantic behavior. Acceptable abstraction boundary, but UI/docs should say “semantic” only if product language requires it.
- **Test file basename order:** `semantic_search_sessions_rpc.rs` (daemon) vs `semantic_session_search_acceptance.rs` (integration) — same words, different order. Low impact; align only if you add more `semantic_*` tests and want predictable sorting.

---

## Function length and complexity

| Area | Assessment |
|------|------------|
| `merge_worktree_label` / `merge_branch_label` | Short, linear; **structural duplication** only. |
| `embed_document`, `relevance_score` | Focused; `relevance_score` combines cosine + lexical boosts in one place (good cohesion). |
| `ensure_schema` | Does version read, conditional migration DDL, and `user_version` bump — **one responsibility** (migrate index) but mixes “read version” and “apply migration” in one block; still readable at ~35 lines. |
| `index_session_for_search` | ~75 lines: I/O, changeset read, embedding, SQL upsert — acceptable; logging is proportional. |
| `search_sessions_semantic` | **Largest hotspot** (~110 lines): open DB, ensure schema, empty-query fast paths, **full-table scan** loop, model/dim checks, floor threshold, sort. Not unmaintainable, but the loop body (row → blob → score → push) is the natural split point if this grows. |

**Complexity notes**

- `MIN_BEST_SCORE` floor is a clear guard against low-signal results; good.
- Sort uses `partial_cmp` with `unwrap_or(Equal)` — standard for `f32`; NaNs are unlikely from this pipeline but if they ever appear, ordering becomes unstable (edge case only).

---

## Duplication

1. **`merge_worktree_label` and `merge_branch_label`** — identical control flow (prefer explicit trimmed string, else suggestion). Small DRY refactor: private helper taking two `Option<&str>` (or `Changeset` accessors) would remove ~15 duplicated lines without changing behavior.

2. **Acceptance tests** (`semantic_session_search_acceptance.rs`) — repeated `TDDY_SESSIONS_DIR_ENV` / `set_tddy_data_dir_override` save-restore blocks across three tests. A local `struct EnvGuard` or `with_test_data_dir(|| { ... })` would cut noise and reduce copy-paste mistakes.

3. **Daemon RPC test** (`semantic_search_sessions_rpc.rs`) — `test_config` / `test_service` pattern is similar to other daemon tests; only worth deduplicating if a shared test helper crate already exists for this workspace.

---

## SOLID: index vs search vs RPC

| Layer | Responsibility | Verdict |
|-------|----------------|---------|
| **`tddy_core::session_semantic_search`** | SQLite location, schema/migration, embedding, indexing, scoring, ranking | **Single module** with multiple cohesive concerns (index + search + embedding). Acceptable for a feature-sized module; boundaries between `index_session_for_search` and `search_sessions_semantic` are clear. |
| **`ConnectionServiceImpl::search_sessions`** | Auth, resolve OS user, resolve `sessions_base`, trim query, call core, map to proto | **Thin adapter** — good separation: no business logic duplicated. |
| **`SessionSearchInput`** | Local state, debounce, callback | **Presentation only** — no RPC knowledge; appropriate. |

No large SRP violation; the main “could split later” candidate is extracting **embedding + relevance** into a submodule if you add alternative backends or unit-test scoring in isolation.

---

## Documentation and comments vs noise

**High value**

- Module-level doc in `session_semantic_search.rs` (paths, schema version, embedding model id, hashing-trick description, recovery story) — **excellent** for operators and future migrations.

**Acceptable**

- Targeted `log::debug!` / `log::info!` with `target: "tddy_core::session_semantic_search"` — aids production debugging without cluttering the TUI (per project rules).

**Noise / drift risk**

- `session_semantic_search_unit.rs` and some integration test comments use **“RED:”** (TDD phase markers). If tests are green and stable, renaming to neutral descriptions (or removing the prefix) avoids implying unfinished work.

- `SessionSearchInput` JSDoc correctly references wiring to `SearchSessions`; keep in sync if the RPC name or transport changes.

---

## Rust idioms

**Good**

- `map_sqlite` centralizes error mapping to `WorkflowError::SessionSearchIndex`.
- `blob_to_embedding` validates length before `chunks_exact`.
- `OpenFlags` and `ensure_schema` follow common rusqlite patterns.

**Minor improvements**

- **`relevance_score`** lowercases `query` and `doc_text` on every call; in the search loop, `query` lowercasing could be done once per search (same for `q_tokens` if `query` is unchanged). Document-text lowercasing is per row by necessity unless cached.
- **Full scan** — appropriate for small indexes; if session count grows, this becomes the bottleneck (not a style issue; document in module doc or a `// perf:` note if relevant).

---

## TypeScript / React idioms

**Good**

- `type="search"` and `aria-label` support accessibility.
- Debounce via `useEffect` + cleanup is idiomatic; `skipInitialDebounceRef` avoids firing on mount with empty string — correct UX.

**Call-site contract**

- `onSearchQuery` is in the effect dependency array. Parents should **`useCallback`** stable handlers to avoid resetting the timer on every parent render. The component doc could add one line: “stabilize `onSearchQuery` with `useCallback` when the parent re-renders often.”

---

## Suggested refactors (actionable, no large rewrites)

1. **DRY `merge_worktree_label` / `merge_branch_label`** — single internal helper; preserve public function names for API stability.

2. **Extract row-scoring block** in `search_sessions_semantic` into a private `fn score_row(...) -> Option<(f32, SessionSearchHit)>` (or return `Result`) to shrink the main function and clarify the `continue` paths.

3. **Comment at proto mapping** in `connection_service.rs** — `SearchSessionHit` ↔ `SessionSearchHit` field mapping is obvious but naming differs; one line avoids confusion.

4. **Test helper** for env override + restore in `semantic_session_search_acceptance.rs` — reduces duplication and clarifies test intent.

5. **Micro-optimization (optional):** hoist `query.to_lowercase()` and `tokenize(query)` out of `relevance_score` when called in the search loop (pass precomputed slices or a small `QuerySignals` struct).

6. **Trim “RED:” prefixes** in unit/integration test comments if the phase is complete.

---

## Priority list

| Priority | Item |
|----------|------|
| **P1** | Hoist per-query work out of `relevance_score` hot path (or document why not worth it) — cheap win if profiling shows cost. |
| **P2** | DRY merge helpers for worktree/branch labels — clarity + less drift risk. |
| **P2** | Add mapping comment `SearchSessionHit` / `SessionSearchHit` at RPC boundary. |
| **P3** | Extract `score_row` (or similar) from `search_sessions_semantic` — readability before adding features (filters, pagination). |
| **P3** | Acceptance test env-restore helper — noise reduction. |
| **P4** | Align test file naming (`semantic_session_search_*`) if the suite grows. |
| **P4** | Document `useCallback` expectation for `SessionSearchInput` consumers. |

---

*Generated as a static review; no executable verification run for this report.*
