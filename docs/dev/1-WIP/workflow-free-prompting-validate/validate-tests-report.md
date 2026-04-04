# Validate-tests report — Worktrees feature

**Date:** 2026-04-04

**Scope:** Validates the **Worktrees manager** work in this branch (`packages/tddy-daemon/src/worktrees.rs`, `packages/tddy-daemon/tests/worktrees_acceptance.rs`, `packages/tddy-web` `WorktreesScreen` + Cypress `WorktreesScreen.cy.tsx`). Focus: **`tddy-daemon`** tests and the **Worktrees** Cypress component spec (per validate subagent tasks).

---

## Commands run

All commands from repository root  
`/var/tddy/Code/tddy-coder/.worktrees/feature-web-worktrees`.

| # | Command | Exit code |
|---|---------|-----------|
| 1 | `./dev cargo test -p tddy-daemon --no-fail-fast` | **0** |
| 2 | `./dev bash -c 'cd packages/tddy-web && bunx cypress run --component --spec cypress/component/WorktreesScreen.cy.tsx'` | **0** |

**Not run in this pass:** full workspace `./test`, `./verify`, or `cargo test --workspace` (optional broader validation deferred).

---

## Results summary

### 1. `cargo test -p tddy-daemon --no-fail-fast`

| Target | Outcome | Count |
|--------|---------|-------|
| Unit tests (`lib.rs`) | **PASS** | **39** passed |
| Binary tests (`main.rs`) | **PASS** | **0** tests |
| `acceptance_daemon` | **PASS** | **8** passed |
| `delete_session` | **PASS** | **2** passed |
| `grpc_spawn_contract` | **PASS** | **1** passed |
| `list_agents_allowlist_acceptance` | **PASS** | **4** passed |
| `list_sessions_enriched` | **PASS** | **1** passed |
| `multi_host_acceptance` | **PASS** | **5** passed |
| `session_workflow_files_rpc` | **PASS** | **3** passed |
| `signal_session` | **PASS** | **3** passed |
| **`worktrees_acceptance`** | **PASS** | **2** passed |
| Doc-tests | **PASS** | **0** |

**Aggregate:** **68** tests reported across non-doc targets (including **39** library unit tests + **29** integration tests in separate binaries). **Exit code 0.**

**Worktrees-specific tests:**

- **Library (`worktrees::tests`):** parsing (`git worktree list`), path validation under repo root.
- **`worktrees_acceptance` integration:** `stats_cache_persists_and_is_served_without_re_diff_on_each_list_call`, `remove_worktree_drops_listing_and_repeat_fails` (uses real `git` in temp repos; prints `git version` and branch-name hints — **not failures**).

### 2. Cypress component — `WorktreesScreen.cy.tsx`

| Metric | Value |
|--------|--------|
| Outcome | **PASS** |
| Tests | **1** passing (`cypress_worktrees_screen_renders_menu_and_table_with_mocked_clients`) |
| Specs | 1 found |
| **Exit code** | **0** |

**Environment notes (non-fatal):** DBus `org.freedesktop.DBus` message during Cypress startup; `resize: can't open terminal /dev/tty` in headless CI-like environment. Neither blocked the run.

---

## Failures

**None.** No failing tests; both commands exited **0**.

---

## Coverage gaps / missing tests (vs PRD-style expectations)

References: PRD mentions in code (`worktrees.rs`, `worktrees_acceptance.rs` — “Worktrees manager PRD”); product docs [web-terminal.md](../../../ft/web/web-terminal.md) (project / worktree path matching); [grpc-remote-control.md](../../../ft/coder/grpc-remote-control.md) (session worktree creation in `tddy-coder` / workflow).

1. **Connect-RPC / `ConnectionService` integration**  
   `packages/tddy-service/proto/connection.proto` (as of this branch) does **not** define list/delete worktree RPCs for the web dashboard. Daemon `worktrees` logic is exercised via **direct library calls** in `worktrees_acceptance.rs`, not via a live gRPC/Connect-RPC handler test. **Gap:** end-to-end RPC tests for auth, project resolution, and error mapping once proto + `connection_service` wiring land.

2. **Web app: routing, shell, and live data**  
   `WorktreesScreen.cy.tsx` mounts a **harness** with **injected `worktrees` props** and no real transport. **Gap:** tests for app route to Worktrees, **Connect** client calls (list/refresh/delete), loading/error states, and integration with **project selection** / `ListProjects`.

3. **Scheduler / workflow**  
   Session worktree creation after plan approval lives in **`tddy-core` / `tddy-service` / workflow** paths (see `setup_worktree_for_session`, `grpc-remote-control.md`). **Gap:** not covered by the Worktrees manager UI/daemon-helper tests; remains validated by existing workflow/daemon tests elsewhere.

4. **Multi-host / cross-daemon**  
   [web changelog](../../../ft/web/changelog.md) notes deferred cross-daemon behavior. **Gap:** no worktrees-specific multi-host tests (expected until routing is productized).

5. **`refresh_stats_for_project` error paths**  
   Unit/integration tests cover happy paths for cache and remove; **limited** automated coverage for `git` failures, corrupt cache JSON, or edge cases in `parse_git_worktree_list_line` (unrecognized lines are logged and skipped — no dedicated negative-test matrix).

6. **Optional broader suite**  
   Full **`./test`** / workspace **`cargo test`** was **not** executed in this pass; run before merge if policy requires full-repo green.

---

## Hygiene

- Cypress emitted **Git “default branch name” hints** during `worktrees_acceptance` tests; informational only.
- Consider committing only intentional artifacts; stray `.*-test-output.txt` files at repo root (if present) should stay untracked unless part of the workflow.
