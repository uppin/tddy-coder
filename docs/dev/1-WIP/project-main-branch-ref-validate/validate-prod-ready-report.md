# Validate Production Ready — Integration Base Ref / Worktree

**Scope:** Branch changes for integration-base ref validation, fetch/worktree helpers, and project registry resolution (`main_branch_ref`, `effective_integration_base_ref_for_project`).  
**Inputs reviewed:** `docs/dev/1-WIP/project-main-branch-ref-validate/evaluation-report.md`, `packages/tddy-core/src/worktree.rs`, `packages/tddy-daemon/src/project_storage.rs`, call sites in `packages/tddy-service/src/daemon_service.rs` and `packages/tddy-workflow-recipes/src/tdd/hooks.rs`.

## Executive summary

The **core library** implements solid validation for the integration-base ref string, structured `git` invocation for fetch, and observable logging. **Project storage** validates `main_branch_ref` on write and resolves an effective ref for lookup.

**Production readiness is partial:** registry-backed `main_branch_ref` is **not** used where worktrees are actually created (`daemon_service`, TDD `hooks`). Sessions still call `setup_worktree_for_session(repo, session_dir)`, which only uses `resolve_default_integration_base_ref` plus a second targeted fetch. That matches the evaluation report’s **high-severity PRD gap** and leaves **configuration and runtime behavior disconnected**.

**Risk level (overall):** **medium** — safe for incremental rollout of validation and heuristics, but incomplete for “per-project base ref drives worktree” as an end-to-end product guarantee.

---

## Alignment with evaluation report

| Finding | Status in this review |
|--------|------------------------|
| `main_branch_ref` not threaded into daemon / hooks | **Confirmed** — only `setup_worktree_for_session` two-arg form is used. |
| `effective_integration_base_ref_for_project` unused on worktree path | **Confirmed**. |
| `resolve_default_integration_base_ref` behavior change (master vs main vs HEAD) | **Confirmed** — operational semantics change for sessions without project override. |
| Double fetch | **Confirmed** — see Performance. |
| Integration test does not prove `projects.yaml` drives HEAD | **Confirmed** — remains an observability / contract gap. |

---

## Error handling

### `tddy-core` (`worktree.rs`)

- **`validate_integration_base_ref`:** Returns `Result<(), String>` with specific messages (empty, prefix, nested segments, whitespace, forbidden characters, `--`).
- **`fetch_integration_base`:** Validates before IO; maps spawn errors and non-zero exit to `String` errors including stderr; does not leak partial success.
- **`resolve_default_integration_base_ref`:** Fails closed if `git fetch origin` fails; fails with a single clear message if no `origin/master`, `origin/main`, or resolvable `origin/HEAD`.
- **`setup_worktree_for_session` / `_with_integration_base`:** Propagate errors from changeset read/write, fetch, and `create_worktree_with_retry`; worktree path is only returned on full success.
- **`remove_worktree`:** On `git worktree remove` failure, logs and falls back to `remove_dir_all` if the path exists — **operational behavior** to avoid stuck state; callers should be aware worktree metadata might be inconsistent if git failed for a reason other than “already gone.”

### `tddy-daemon` (`project_storage.rs`)

- **`add_project`:** Invalid `main_branch_ref` fails before persisting; uses `anyhow` with context.
- **`effective_integration_base_ref_for_project`:** Unknown `project_id` is an error; invalid stored ref (if YAML edited by hand) is caught at resolve time via `validate_integration_base_ref`.

### Call sites

- **`daemon_service::handle_approve_session_document`:** Worktree failure is turned into `send_workflow_complete(..., false, e)` — user-visible failure, no silent fallback.
- **`hooks::ensure_worktree_for_acceptance_tests`:** Failure returns `Err` with message; **`log::error!`** records repo root, session dir, and error (good for ops triage).

---

## Logging (levels and appropriateness)

| Location | Level | Assessment |
|----------|-------|------------|
| `fetch_integration_base` | `info` start, `debug` success/failure detail | Reasonable: `info` may be noisy on every session; acceptable for auditability; consider `debug` for high-frequency deployments. |
| `resolve_default_integration_base_ref` | `info` on fetch, `debug` on chosen ref | Same tradeoff. |
| `setup_worktree_for_session*` | `info` at entry, `debug` for resulting path | Consistent. |
| `effective_integration_base_ref_for_project` | `debug` lookup, `info` when ref or default chosen | Appropriate; avoids leaking full YAML in logs. |
| `add_project` | `info` with `project_id` | Appropriate. |
| Hooks on worktree failure | `error!` | Appropriate for production diagnostics. |

**Gap:** When the product eventually threads `project_id` into worktree setup, logging should include **which ref was applied** (explicit vs default vs resolved) in one place to simplify support.

---

## Configuration

- **`ProjectData.main_branch_ref`:** Optional; documented default when absent is `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF` (`origin/master`).
- **Persistence:** `projects.yaml` via serde; invalid refs rejected on `add_project` and re-validated in `effective_integration_base_ref_for_project`.
- **RPC / create project:** `connection_service` builds `ProjectData` with **`main_branch_ref: None`** always — new projects from this path do not set a per-project base ref via API; operators must edit YAML or a future API must supply it.

**Configuration–runtime disconnect:** Daemon and workflow hooks do not read `effective_integration_base_ref_for_project` when creating worktrees, so **`main_branch_ref` in `projects.yaml` does not affect** `setup_worktree_for_session` today.

---

## Security (git command construction and injection)

### `validate_integration_base_ref` + `fetch_integration_base`

- Ref must be exactly `origin/<one-segment>` with no shell metacharacters, whitespace, or `--`.
- Fetch uses `Command::new("git").args(["fetch", "origin", branch])` where `branch` is the segment after `origin/` — **no shell**, arguments are discrete, and the branch name is constrained by validation.

**Residual considerations:**

- **`create_worktree`** passes `start_point` as `Option<&str>` from either a validated integration ref or from **`branch` / `worktree` strings from the changeset** (planning output). Those strings are **not** subject to the same `validate_integration_base_ref` rules. If an attacker or malformed planner could supply `branch_suggestion` with `--` or odd tokens, `git worktree add` could interpret unexpected arguments. This is **pre-existing surface area** but worth hardening if branch names ever come from untrusted input (e.g. strict slug validation for branch names).
- **`worktree_path.to_str().unwrap()`** in `create_worktree` can **panic** on non–UTF-8 paths — low probability on typical Unix deployments, but a denial-of-service or crash edge case if paths are exotic.

---

## Performance

### Double fetch (`setup_worktree_for_session`)

1. `resolve_default_integration_base_ref` runs **`git fetch origin`** (full default remote fetch).
2. `setup_worktree_for_session_with_integration_base` calls **`fetch_integration_base`**, which runs **`git fetch origin <branch>`**.

For a typical default resolution (`origin/master` or `origin/main`), the second fetch is often cheap but **redundant** after a full `fetch origin`. Impact: extra latency and load on large repos or slow networks; **not** typically a correctness bug.

**Mitigation direction (non-blocking for “prod ready” doc):** skip the second fetch when the resolved ref was already updated by the first fetch, or pass a flag / reuse ref resolution without an extra network round-trip.

---

## Operational risks

1. **Semantic change:** Sessions without a project-specific ref now follow **master → main → `origin/HEAD`** instead of always `origin/master`. Repos with both `master` and `main` will prefer **`origin/master`**, which may or may not match operator expectations for “default integration branch.”
2. **Network dependency:** Every worktree setup performs at least one remote fetch; failures surface as workflow/session errors (handled, but user-visible).
3. **Registry vs runtime:** Operators may set `main_branch_ref` in YAML and assume worktrees track it — **currently false** until daemon/hooks call `setup_worktree_for_session_with_integration_base` with `effective_integration_base_ref_for_project` (or equivalent). Risk: **misconfiguration and confusion**, not silent wrong git state from YAML alone.
4. **Stub backend (hooks):** Demo/stub path skips git fetch — correct separation; production paths use real git.

---

## Call-site gaps (`daemon_service.rs`, `tdd/hooks.rs`)

| File | Current behavior | Gap |
|------|------------------|-----|
| `daemon_service.rs` | `handle_approve_session_document` calls `setup_worktree_for_session(repo, session_dir_path)` only; comment still says “origin/master”. | No `project_id` → no `effective_integration_base_ref_for_project`; cannot honor `main_branch_ref`. |
| `tdd/hooks.rs` | `ensure_worktree_for_acceptance_tests` calls `setup_worktree_for_session(&repo_root, session_dir)` for non-stub backends. | Same; comments still say “origin/master”. |

**`setup_worktree_for_session_with_integration_base` exists** and is the right primitive once session/project binding is available in these layers.

---

## Production readiness verdict

| Area | Verdict |
|------|---------|
| Validation + fetch for **explicit** `origin/<branch>` | **Ready** — appropriate checks and command construction. |
| Default ref resolution | **Ready** with **documented behavior change** risk for mixed-branch remotes. |
| Project registry API (`add_project`, `effective_integration_base_ref_for_project`) | **Ready** for storage and lookup. |
| End-to-end use of `main_branch_ref` for worktree creation | **Not ready** — daemon and workflow hooks do not consume it. |
| Performance (double fetch) | **Acceptable** with known optimization opportunity. |
| Security (validated ref path) | **Strong**; branch/worktree name validation in `create_worktree` remains a broader hardening topic. |

**Recommendation:** Treat as **production-ready for** validation, default-resolution heuristics, and YAML-safe persistence. Treat as **not yet production-complete for** the full “per-project integration base drives worktree” story until call sites pass the effective ref and add integration coverage that proves `projects.yaml` controls worktree HEAD.

---

## Confirmation

- **Report path:** `docs/dev/1-WIP/project-main-branch-ref-validate/validate-prod-ready-report.md`
- **Written:** yes (this file).
