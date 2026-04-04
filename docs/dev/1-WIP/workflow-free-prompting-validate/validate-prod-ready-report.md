# Validate prod-ready report: worktrees manager (partial PRD)

## Executive summary

The worktrees module in `packages/tddy-daemon/src/worktrees.rs` is a **library-only** slice of the Worktrees manager PRD: git subprocess calls, a JSON stats cache under **`TDDY_PROJECTS_STATS_ROOT`** (default `~/.tddy/projects`), lexical path policy, and **`remove_worktree_under_repo`** with primary-worktree protection. **There is no daemon RPC wiring yet** — the acceptance test file is named `worktrees_acceptance.rs` but exercises the library directly, not Connect/gRPC handlers. The web **`WorktreesScreen`** is **mock-data / Cypress-only** with an explicit comment that RPC is not connected.

**Production readiness:** suitable as an **internal building block** with clear follow-ups before calling it “done” for daemon deployment: structured errors and surfacing for refresh/list cache paths, log level hygiene, git subprocess timeouts and `PATH` assumptions, and tightening path policy where symlinks matter. **Residual risk: medium** until RPC, authz, and operational controls are defined.

**Evaluation context (aligned):**

- Partial PRD — no RPC surface in daemon server code for list/refresh/delete.
- Lexical path validation — no `canonicalize` / filesystem bind; `..` traversal is rejected relative to normalized repo root.
- Cache under `TDDY_PROJECTS_STATS_ROOT` — documented override for tests; default uses `HOME` (panics if unset).
- Git subprocess — `Command::new("git")` with no configurable binary path or timeouts.

---

## Error handling

| Area | Behavior | Prod concern |
|------|-----------|--------------|
| `parse_git_worktree_list` | Returns `Vec`; unrecognized lines → `warn!` + skip | Silent data loss in list; acceptable if UI treats partial as degraded |
| `refresh_stats_for_project` | **`()` — no `Result`** | Git `worktree list` failure → empty string → **empty snapshot persisted** (or overwrite with empty). Spawns warn logs but **callers cannot distinguish success from total failure** |
| `git_diff_numstat_summary` | **`(0,0,0)` on any failure** | Stats silently zero; no distinction “repo missing” vs “clean tree” |
| `directory_size_bytes_best_effort` | Ignores read/metadata errors | Disk size can be **under-reported** with no signal |
| `list_cached_stats` | Missing file → `[]`; read/parse error → `warn!` + `[]` | **Stale-empty vs error** indistinguishable to consumers |
| `remove_worktree_under_repo` | **`Result<_, RemoveWorktreeError>`** | Strongest API: git failures, not listed, primary protected, UTF-8 path |
| `validate_worktree_path_within_repo_root` | **`Result<_, WorktreePathError>`** | Good for RPC pre-checks once wired |
| `projects_stats_cache_root` | **`HOME` unset → `expect` panic** | Breaks minimal/containers without `HOME`; should be `Result` or explicit error for prod |

**Git stderr:** Not captured on failed `git worktree list` in `refresh_stats_for_project` (only `status` in `remove_worktree_under_repo` path is partially surfaced). Debugging production failures will rely on warn lines without stderr text.

---

## Logging

- Uses **`log::{debug, info, warn}`** — consistent with daemon style; no `println!` in this module.
- **Noise risk:** Successful paths log at **`info!`** for routine operations (`parse_git_worktree_list` row count, `list_cached_stats` snapshot count, `validate_worktree_path_within_repo_root` success, `WorktreeStatsCache::new`, `refresh_stats_for_project` row counts). Under default **info** level in a busy multi-project daemon, this can **flood** logs on every list/refresh.
- **Recommendation:** Prefer **`debug!`** for per-request success paths; reserve **`info!`** for state changes (cache write, worktree removed, policy rejection).

**Web:** `WorktreesScreen` uses **`console.error`** for `logTddyMarker("M009", ...)` on every render — **noisy in browser consoles** if this ships unchanged in production builds.

---

## Configuration

| Mechanism | Purpose |
|-----------|---------|
| **`TDDY_PROJECTS_STATS_ROOT`** | Overrides cache root (integration tests, custom data dir). Logged at **info** when set (path value visible in logs). |
| **`HOME`** | Default cache: `~/.tddy/projects`. **Required** for default branch; otherwise panic. |

**Gaps:** No daemon config file key for stats root (env-only). No separate toggle for “enable worktrees feature.” **`git` binary** is hardcoded — no `GIT_BINARY` / config override for Nix or minimal images.

---

## Security

**Path policy (`validate_worktree_path_within_repo_root`):**

- **Lexical** normalization only — **does not resolve symlinks**. A symlink under `repo_root` pointing outside could differ from lexical containment in edge cases depending on OS and deployment.
- **`starts_with` on `PathBuf`** — acceptable for Unix; Windows prefix/canonical path edge cases need review if that platform is supported.
- **Traversal:** `../../../etc/passwd` style paths are rejected when joined to repo root (test coverage in-module).

**Removal (`remove_worktree_under_repo`):**

- **Authority = `git worktree list` output** parsed by the same parser as refresh — path must be listed for `repo_root` and must **not** match the **first** row (primary). Comment notes worktrees may live **outside** the main directory; removal is still constrained to git’s notion of registered worktrees, not arbitrary paths.
- **No `--force`** in current `git worktree remove` invocation — safer default; dirty trees may fail with git error (surfaced as `GitFailed`).
- **UTF-8:** Non–UTF-8 paths cannot be passed to `git` args (error returned).

**Cache on disk:** JSON under per-`project_id` directory with `project_id` sanitized (`/`, `\`, `:` → `_`). No encryption; world-readable if umask allows — **same class of risk as other `~/.tddy` data**.

**RPC/authz:** Not implemented — **who may refresh or delete** is undefined at the transport layer.

---

## Performance

- **Refresh (`refresh_stats_for_project`):** One `git worktree list`, then **per worktree**: full **directory tree walk** for size + **`git diff --numstat HEAD`**. Cost grows **linearly with worktree count** and with tree size; **no parallelism cap**, no batching, no incremental diff.
- **List (`list_cached_stats`):** Single file read + JSON parse — **does not** call `git_diff_numstat_summary` (by design). Acceptance test asserts diff counter does not increase on repeated list calls.
- **Note:** `test_git_diff_on_list_calls` in `WorktreeStatsCache` is **never incremented** in `worktrees.rs`; the invariant is enforced structurally (list path does not call diff), not by the atomic counter.

---

## Operational gaps (daemon deployment)

1. **No RPC/handlers** — Feature not exposed through existing daemon server; clients cannot use it without new protobuf/Connect messages and wiring.
2. **`git` on `PATH`** — Service units must ensure git is installed; no preflight or clear startup error if missing.
3. **No subprocess timeouts** — Hung `git` could block a worker thread indefinitely.
4. **Resource limits** — No max worktrees per project, no max refresh frequency, no disk quota on cache directory.
5. **Observability** — No metrics (refresh duration, cache hit rate, git failure count); logs only.
6. **Cache coherence** — After `remove_worktree_under_repo`, **callers must** call `invalidate_project` (or refresh) if the cache should drop stale rows; **not automatic** from `remove_worktree_under_repo`.
7. **Test name accuracy:** Renamed to **`remove_worktree_drops_listing_and_repeat_fails`** (library-level remove, not RPC). Extend when RPC + cache integration exists.

---

## Web + Cypress

- **`WorktreesScreen.tsx`:** Presentational; **mock rows only**; comment states daemon/RPC future.
- **`WorktreesScreen.cy.tsx`:** Component test with **injected props** — explicitly **no live RPC** (per spec comment). Adequate for UI shell; **not** an E2E against daemon.

---

## Verdict

| Dimension | Status |
|-----------|--------|
| Error handling (remove/validate) | **Good** — typed errors |
| Error handling (refresh/list cache) | **Weak** — silent empty/stale, no `Result` on refresh |
| Logging | **Functional** — tune levels for prod |
| Configuration | **Minimal** — env + `HOME`; document and harden |
| Security (remove + lexical policy) | **Reasonable baseline** — symlink/canonical edge cases + authz TBD |
| Performance (list vs refresh) | **List path OK**; **refresh can be heavy** |
| Daemon deployment | **Not ready** — RPC, timeouts, HOME, observability, cache invalidation contract |

**Suggested next steps before prod:** Wire RPC with authz; return `Result` or structured status from refresh; downgrade routine logs to `debug`; add git timeouts; replace `HOME` panic with explicit error; document cache invalidation after delete; consider `git` binary override for controlled environments.
