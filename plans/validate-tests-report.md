# Validate-tests report: semantic session search

**Repository:** `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search`  
**Run window:** started `2026-04-05T11:52:41Z` (UTC), commands executed sequentially in this subagent environment.

## Commands run

All commands were run from repo root: `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search`.

| # | Command | Result |
|---|---------|--------|
| 1 | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev cargo test -p tddy-core --test session_semantic_search_unit` | **PASS** — 6 tests |
| 2 | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev cargo test -p tddy-integration-tests --test semantic_session_search_acceptance` | **PASS** — 3 tests |
| 3 | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev cargo test -p tddy-daemon --test semantic_search_sessions_rpc` | **PASS** — 1 test |
| 4 | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev bun run --filter tddy-web cypress:component -- --spec cypress/component/SessionSearchInput.cy.tsx` | **PASS** — 1 spec, 1 passing test (`web_search_input_triggers_query_debounced`) |
| 5 (optional) | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev cargo test -q -p tddy-core -p tddy-daemon 2>&1 \| tail -30` | **FAIL** (compile) — see below |
| 6 (follow-up) | `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./dev cargo test -q -p tddy-core` | **PASS** — full `tddy-core` package tests |

### Failure excerpt (command 5)

`cargo test -p tddy-core -p tddy-daemon` did not complete: `tddy-daemon` test target `sessions_base_path_mismatch` fails to compile:

```text
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
  --> packages/tddy-daemon/tests/sessions_base_path_mismatch.rs:57:5
...
ConnectionServiceImpl::new(config, sessions_base_resolver, user_resolver, None, None)
...
help: provide the argument ... Option<Arc<TelegramDaemonHooks>>

error[E0063]: missing field `pending_elicitation` in initializer of `SessionMetadata`
  --> packages/tddy-daemon/tests/sessions_base_path_mismatch.rs:72:20
```

**Note:** Full `./test` was not run; focused semantic-search commands and optional broader `tddy-core`/`tddy-daemon` sweep were used instead.

### Cypress noise (non-fatal)

The component run logged DBus/`resize` warnings and port fallback (5173→5175); exit code was 0 and the spec passed.

---

## Missing coverage: PRD / product intent vs tests

Inferred from test module comments (`semantic_session_search_acceptance.rs` references a “PRD Testing Plan”), module docs in `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/packages/tddy-core/src/session_semantic_search.rs`, and code search.

| Area | What tests cover today | Gap |
|------|------------------------|-----|
| **Index schema / embedding metadata** | Unit tests assert published model id and schema version (`session_semantic_search_unit`). | Low gap for constants; migration stress not covered beyond version constant. |
| **Indexing + ranking + incremental refresh** | Integration acceptance: persist index path, semantic ranking, re-index after changeset update (`semantic_session_search_acceptance`). | Indexing is invoked **explicitly in tests**; production daemon code paths do not call `index_session_for_search` (grep shows usage only in `tddy-core` + tests). **Search** is wired in `connection_service.rs` via `search_sessions_semantic`; **automatic re-index on session/changeset updates** is not verified in production code by these tests. |
| **RPC / API contract** | Daemon test seeds index via `index_session_for_search`, then asserts `SearchSessions` behavior and stable schema (`semantic_search_sessions_rpc`). | Does not cover auth edge cases beyond what the single test asserts; no live Connect-RPC E2E against a running daemon in CI described here. |
| **Web UI** | Cypress **component** test debounced query behavior (`SessionSearchInput.cy.tsx`). | No Cypress **e2e** (full app + `/rpc` proxy + real daemon) for search; no visual regression for search results list. |
| **Load / performance** | None in this run. | No load tests for large session counts, index size, or concurrent searches. |
| **Indexer hooks in prod** | N/A in strict sense — tests prove library + manual indexing. | **Gap:** ensure every production write path that updates searchable session state (e.g. changeset save, session lifecycle) calls `index_session_for_search` or equivalent batch job; otherwise search results drift from disk. |

---

## Recommendations

1. **Fix `sessions_base_path_mismatch` test compile errors** in `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/packages/tddy-daemon/tests/sessions_base_path_mismatch.rs`: pass the sixth `ConnectionServiceImpl::new` argument and populate `SessionMetadata.pending_elicitation` so `./dev cargo test -p tddy-daemon` (and combined `-p tddy-core -p tddy-daemon`) is green again.
2. **Define and implement production indexing hooks** (or a documented background reindex job) and add an integration test that exercises the **same code path** the daemon uses after a changeset update, not only direct `index_session_for_search` calls from tests.
3. **Add an E2E or daemon-level test** that starts the stack (or minimal gRPC server) and validates `SearchSessions` against a persisted index after a simulated workflow write, if product requires parity with local dev (`web-dev`).
4. **Optional:** performance smoke test (e.g. N sessions, query latency bound) once indexing is production-bound.
5. **Re-run** `/var/tddy/Code/tddy-coder/.worktrees/semantic-session-search/./verify` or `./test` after fixing daemon tests for full-workspace confidence.

---

*Generated by validate-tests subagent; absolute paths preserved as requested.*
