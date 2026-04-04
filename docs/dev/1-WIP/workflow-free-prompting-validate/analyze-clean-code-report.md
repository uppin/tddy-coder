# Clean-code analysis: Git worktrees (daemon + web + tests)

**Scope:** `packages/tddy-daemon/src/worktrees.rs`, `packages/tddy-web/src/components/worktrees/WorktreesScreen.tsx`, `packages/tddy-daemon/tests/worktrees_acceptance.rs`, `packages/tddy-web/cypress/component/WorktreesScreen.cy.tsx`.

## Summary

The Rust module is **well-documented at the crate level**, uses **clear domain types** (`WorktreeListRow`, `WorktreeStatSnapshot`, typed errors), and keeps **git parsing** separated from **cache persistence** and **removal policy**. The React screen is a **small presentational component** with predictable `data-testid` hooks. The main risks are **naming drift** between Rust/TS DTOs once RPC is wired, a **misleading integration test name** that does not match what it exercises, **render-time side effects** for debug markers, and **one large method** on `WorktreeStatsCache` that mixes orchestration and I/O.

---

## `worktrees.rs`

### Strengths

- **Naming:** Consistent `Worktree*` prefix; verbs match behavior (`parse_git_worktree_list`, `validate_worktree_path_within_repo_root`, `remove_worktree_under_repo`).
- **Safety:** Lexical normalization for path policy avoids filesystem-dependent behavior for traversal checks; primary-worktree removal is blocked explicitly.
- **Observability:** Structured `log` usage with function-prefixed messages aids grep-based debugging.
- **Tests:** Unit tests cover parser fixtures and path policy; acceptance behavior is pushed to `tests/worktrees_acceptance.rs`.

### Issues

| Area | Observation |
|------|----------------|
| **SRP / length** | `refresh_stats_for_project` runs git list, walks rows, stat/diff per row, increments test atomics, serializes JSON, and writes disk—**~70 lines** in one method. Harder to unit-test pieces without subprocesses. |
| **Duplication** | `git worktree list` is invoked similarly in `refresh_stats_for_project` and `remove_worktree_under_repo` (spawn, `current_dir`, handle failure). A private **`git_worktree_list_output(repo_root) -> Result<String, …>`** would centralize behavior and error mapping. |
| **API surface** | `WorktreeListRow.lock_path` is always `None` in `parse_git_worktree_list_line`. Either **parse lock files** when needed, **remove the field** until then, or **document** it as reserved for future porcelain formats. |
| **Naming** | `projects_stats_cache_root` is accurate but long; acceptable. `test_git_diff_*` counters on a production struct are honest for acceptance tests but **blur layering** (consider a test-only wrapper or cfg-gated fields if this grows). |

### Repo Rust style

- Module `//!` and `///` on public items align with typical project style.
- No `println!` in library code (uses `log`)—matches AGENTS.md TUI/daemon guidance.
- `expect("HOME must be set...")` is strict; consistent with “no silent fallback” rules for default cache root.

---

## `WorktreesScreen.tsx`

### Strengths

- **Separation of concerns:** Table is presentational; data and `onConfirmDelete` are injected—easy to swap mock vs RPC-backed clients later.
- **Accessibility:** `<th scope="col">` is used appropriately.
- **Testability:** Stable `data-testid` attributes align with Cypress usage.

### Issues

| Area | Observation |
|------|----------------|
| **Naming** | `WorktreesScreenMockRow` encodes “mock” in the type name—fine for Storybook/Cypress now; when wired to production, consider **`WorktreeTableRow`** (or mirror daemon field names) and keep mocks in stories/tests only. |
| **Field mapping** | Rust uses `branch_label`; TS uses `branch`. A future mapper layer should live in one place (e.g. RPC client) to avoid scattered renames. |
| **Side effects** | `logTddyMarker` runs on **every render** (not `useEffect`). Under React Strict Mode (double render in dev), markers may duplicate—acceptable only if intentional for tracing; otherwise move to `useEffect` with stable deps or document the choice. |
| **Inline UI** | Delete + confirm inline in the row map is still short; if actions grow (e.g. “Open in terminal”), extract a row subcomponent. |

### Repo TS/React style

- Functional component, explicit props interface—consistent with modern React in the repo.
- `console.error` for structured markers matches the comment (“visible in Cypress”); ensure production bundling policy allows or strips these if noisy.

---

## `worktrees_acceptance.rs` (integration tests)

### Strengths

- **Helpers:** `require_git`, `run_git`, `init_repo_with_secondary_worktree`, `worktree_list_contains_path` keep tests readable.
- **Env isolation:** `TDDY_PROJECTS_STATS_ROOT` restored with `scopeguard`—clear pattern for cache root override.

### Issues

| Area | Observation |
|------|----------------|
| **Naming vs behavior** | **`remove_worktree_drops_listing_and_repeat_fails`** matches **`remove_worktree_under_repo`** (library-level; no RPC yet). |
| **File name** | **`worktrees_acceptance.rs`** — library-level acceptance tests (not wire RPC). |

---

## `WorktreesScreen.cy.tsx`

### Strengths

- **Harness pattern:** Local `WorktreesHarness` composes nav + screen + deleted-state—keeps the spec focused.
- **Assertions:** Covers menu visibility, table structure, column headers, row count, and delete confirmation flow.

### Issues

| Area | Observation |
|------|----------------|
| **Test title** | `cypress_worktrees_screen_renders_menu_and_table_with_mocked_clients` is long; if the repo convention is prefixing with `cypress_`, it is consistent—otherwise shorten while keeping uniqueness. |
| **Duplication** | `MOCK_ROWS` mirrors `WorktreesScreenMockRow`—acceptable duplication between test and component contract; if the shape changes often, a shared fixture module (under `cypress/fixtures` or a test-only export) reduces churn. |

---

## Prioritized refactor suggestions

### P0 — Correctness / maintainability (do first)

1. **RPC + cache:** When **`connection_service`** delete + cache invalidation exist, add integration tests that exercise those layers (current tests stay library-level).

### P1 — Structure and DRY

2. **Shared `git worktree list`:** **`git_worktree_list_stdout`** is used by **`refresh_stats_for_project`**; **`remove_worktree_under_repo`** still invokes git inline—optional consolidation later.

3. **Define a single DTO mapping contract** for web when RPC lands: either TS types mirror serde field names (`branch_label`, `disk_bytes`, etc.) or one **`toWorktreesScreenRow(snapshot): WorktreesScreenMockRow`** function in the RPC client module.

### P2 — Readability and layering

4. **`refresh_stats_for_project`** — split into **`git_worktree_list_stdout`**, **`build_worktree_stat_snapshots`**, **`write_worktree_stats_cache_file`** (done in PR wrap).

6. **Resolve `lock_path` vs parser**—implement, remove, or document as placeholder.

7. **`logTddyMarker` in `WorktreesScreen`:** move behind `useEffect` or add a one-line comment that render-time duplication under Strict Mode is acceptable for debugging.

---

## Conclusion

The worktrees feature code is **readable and appropriately layered for an early integration**: Rust boundaries are clear, the UI is thin and test-friendly, and Cypress covers the critical UX path. The highest-impact cleanup is **honest naming in tests and files** (RPC/cache claims vs actual coverage), followed by **small extractions** in `worktrees.rs` and a **planned RPC↔UI field mapping** to prevent silent drift between daemon and web.
