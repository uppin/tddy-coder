# Bugfix Analyze — Production Readiness Validation

**Scope:** `packages/tddy-workflow-recipes` bugfix analyze path (`hooks.rs`, `analyze.rs`, `mod.rs`, `parser.rs` analyze types), `packages/tddy-core/src/backend/stub.rs` analyze response, `goals.json` / `analyze.schema.json`.

**Context:** See `plans/bugfix-analyze/evaluation-report.md` (medium risk; optional `summary`→reproduce merge and hygiene items noted there).

---

## Executive summary

The bugfix **analyze** pipeline is structurally sound for production: structured submit is validated (schema + parser), persistence uses the shared `Changeset` model and `log` (not raw stdout) in hook/analyze paths, and **`after_task` errors propagate** through `FlowRunner` (`runner.rs` uses `?` on `hooks.after_task`), so invalid or empty analyze output fails the step instead of silently advancing.

Remaining production concerns are **mostly operational and product-complete**: optional `summary` is parsed but **not stored** on `Changeset` (no field), so downstream reproduce cannot consume it without further work; **`goal_requires_session_dir` is `false`** while hooks **require `session_dir` for persistence**—if context omits it, analyze completes without writing branch/worktree suggestions; **`next_goal_for_state`** defaults unknown states to **analyze**, which can surprise resume semantics; seeding failures in `before_task` are **logged at debug** and not surfaced as errors. **Security:** no embedded secrets; log lines may include session paths at info/debug (normal for ops, mind shared logs).

**Verdict:** Acceptable for merge with documented follow-ups; no blocker-class security or logging/TUI violations found in the reviewed paths.

---

## Strengths

1. **Error propagation (critical path)** — `apply_analyze_submit_to_changeset` returns `Err` on empty response or parse/read/write failures; `BugfixWorkflowHooks::after_task` propagates that result, and the workflow runner treats hook errors as step failure.

2. **Logging vs stdout** — Hooks and `analyze.rs` use `log::info!` / `log::debug!` only. No `println!`/`eprintln!` on those paths (aligns with TUI safety rules). `println!` appears only in `plain_goal_cli_output` on `BugfixRecipe`, which is the intentional plain-CLI surface for user-facing output.

3. **Parser discipline** — `parse_analyze_response` enforces `goal == "analyze"` and non-empty `branch_suggestion` / `worktree_suggestion`, consistent with embedded JSON Schema (`required` fields, `goal` const).

4. **Schema and registration** — `goals.json` registers `analyze` with `analyze.schema.json` and proto; schema is minimal, explicit, and versioned via manifest (see generated artifacts in repo).

5. **Observability** — Analyze persistence logs task id, **response length** at info (not full body), and structured fields at debug—reasonable balance for production logs.

6. **Stub backend** — `analyze_response` emits deterministic, schema-shaped JSON via existing `submit_and_respond` path; uses `log::debug!` for submit flow; no secrets or env coupling.

7. **Double validation layer** — Runtime path assumes `tddy-tools` schema validation plus recipe-side parsing; defense in depth for malformed agent output.

---

## Gaps / risks

### Error handling

| Issue | Severity | Notes |
|--------|-----------|--------|
| Missing `session_dir` in `after_task` | **Medium (operational)** | Code logs at debug and **returns `Ok(())`**—workflow may advance to reproduce **without** persisted branch/worktree. Conflicts with the expectation that analyze submit is required for naming. |
| `before_task` analyze: `write_changeset` / `read_changeset` failures | **Low** | Errors logged at **debug** only; state may not be `Analyzing` or seed may fail without failing the step. |
| `on_error` is a no-op | **Low** | Consistent with minimal hooks; task failures still surface via runner. No extra bugfix-specific diagnostics. |

### Logging

| Issue | Severity | Notes |
|--------|-----------|--------|
| `session_dir` in log lines | **Low** | `{:?}` on paths can expose machine-specific paths in aggregated logs—acceptable for devops; consider redaction only if logs are exported to untrusted sinks. |
| No `log::target` on bugfix analyze | **Info** | Unlike some `parser` red-path logging, bugfix uses default target—filtering in centralized logging is slightly harder. |

### Configuration / environment

| Issue | Severity | Notes |
|--------|-----------|--------|
| `goal_requires_session_dir` → `false` | **Medium** | Mismatch with hooks that persist to `session_dir`; relies on upper layers always supplying it when a session-backed run is intended. |
| `next_goal_for_state`: default `_ => Some(analyze)` | **Low** | Broad fallback may send resume to **analyze** for unexpected/legacy state strings; can duplicate work or confuse operators (see evaluation report). |

### Security

| Issue | Severity | Notes |
|--------|-----------|--------|
| No hardcoded secrets | — | None observed in reviewed files. |
| User/agent-controlled strings | **Info** | `branch_suggestion` / `worktree_suggestion` flow into `changeset.yaml`; consumers that create filesystem paths must validate (outside this review). |

### Performance

| Issue | Severity | Notes |
|--------|-----------|--------|
| `before_task` analyze: up to **two** `read_changeset` calls | **Low** | Minor I/O duplication on session start. |
| `system_prompt()` allocates large static strings each call | **Info** | Typical for recipe hooks; negligible vs model invocation. |
| Full `task_result.response` in memory | **Info** | Expected; no extra copy beyond parse. |

### Operational

| Issue | Severity | Notes |
|--------|-----------|--------|
| Optional **`summary` not persisted** | **Medium (product)** | Parsed in `AnalyzeOutput` but `apply_analyze_submit_to_changeset` does not map it onto `Changeset` (no `summary` field). Reproduce step does not receive it automatically. |
| `.red-phase-submit.json` untracked artifact | **Low** | Hygiene: should not ship in repo (per evaluation report). |
| Documentation | **Info** | Product/docs changeset may still be pending per evaluation report. |

---

## Recommendations (prioritized)

### P1 — Should do before or immediately after release

1. **Align session directory contract** — Either set `goal_requires_session_dir` to `true` for goals that persist to disk (if the core API allows per-goal behavior without breaking other flows), or document and enforce that bugfix sessions **must** populate `session_dir` before analyze completes; consider returning **`Err` from `after_task`** when `goal_requires_tddy_tools_submit` is true but `session_dir` is missing, so the run fails fast.

2. **Decide on `summary` persistence** — Add a changeset field (or artifact) for analyze summary and thread it into reproduce context, **or** remove `summary` from schema/prompt until implemented—avoids a “dead” schema field in production.

### P2 — Hardening

3. **Surface seed/state write failures** — If `write_changeset` fails during analyze `before_task`, consider **`warn`** or propagating where safe, so operators see partial initialization issues.

4. **Narrow `next_goal_for_state` default** — Map known legacy states explicitly; use a conservative default (or `None` where appropriate) instead of unconditional `analyze` for all unknown strings, to reduce mistaken resumes.

5. **Structured log targets** — Use a dedicated `log::target!` for `tddy_workflow_recipes::bugfix` to simplify filtering in production log pipelines.

### P3 — Nice to have

6. **Single `read_changeset` in `before_task` analyze** — Refactor to one read path to reduce I/O.

7. **`on_error` hook** — Optional `log::warn!` with `task_id` for bugfix to improve supportability without changing failure semantics.

---

## Confirmation

**File written:** `/var/tddy/Code/tddy-coder/.worktrees/bugfix-analyze/plans/bugfix-analyze/validate-prod-ready-report.md`
