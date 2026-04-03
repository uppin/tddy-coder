# Validate prod-ready report: workflow free-prompting / approval policy

## Executive summary

The free-prompting recipe, resolver wiring, CLI/TUI surfaces, and presenter bootstrap (`recipe` in `changeset.yaml`) are implemented coherently for production use: errors surface as `String`/`anyhow` user-facing messages, workflow paths use `log::` (not raw stdout in the workflow-recipes crate), and approval behavior for “no PRD gate” is enforced via **`WorkflowRecipe::uses_primary_session_document`** (false for free-prompting). **Medium residual risk** comes from **manual synchronization** of recipe names across several lists (resolver, `approval_policy`, clap `value_parser`, TUI question mapping), **`approval_policy::recipe_should_skip_session_document_approval` not being referenced from runtime code** (policy vs. trait could drift), **verbose `log::info` of agent output** in free-prompting CLI output handling, and **repo hygiene** (untracked red-test output file; product docs gap noted in the evaluation report). No secrets are introduced in the reviewed paths.

**Cross-reference — `evaluation-report.md`:**  
- **Docs gap:** `workflow-recipes.md` / A4-style product docs not updated in the evaluated diff.  
- **Drift:** `approval_policy` list must stay aligned with `recipe_resolve` (and implicitly with CLI/TUI allowlists).  
- **Hygiene:** `.tddy-workflow-recipes-red-test-output.txt` should not be committed.

---

## Findings by area

### Errors

- **Resolver:** `unknown_workflow_recipe_error` returns a clear `String` listing allowed names via `approval_policy::supported_workflow_recipe_cli_names()` — good UX for CLI/YAML/daemon parity (`recipe_resolve.rs`).
- **CLI:** `validate_recipe_cli` / `recipe_arc_for_args` map resolver errors through `anyhow` for early exit with `eprintln!` in `run_main` — appropriate for non-TUI CLI bootstrap.
- **Presenter bootstrap:** `workflow_runner` propagates failures via `WorkflowEvent::WorkflowComplete(Err(...))` rather than panicking on I/O; `write_changeset` is still invoked with **`let _ =`** — **silent ignore on write failure** remains a risk (recipe might not persist if disk fails); this pattern predates the feature but affects correctness of the new `recipe` field.
- **`approval_policy`:** `recipe_should_skip_session_document_approval` is **only used in tests** (`recipe_policy_red.rs`); runtime approval gating does **not** call it. If someone updates the policy table without updating `FreePromptingRecipe::uses_primary_session_document`, behavior could diverge from the documented “policy table.”

### Logging

- **workflow-recipes:** Uses `log::debug!` / `log::info!` only — no `println!` in reviewed files — consistent with AGENTS.md guidance for non-TUI library code.
- **Free-prompting verbosity:** `FreePromptingRecipe::plain_goal_cli_output` logs the **full optional output body** at **`log::info!`** (`[free-prompting] output:\n{}`). In production this can mean **large or sensitive agent text in logs**; consider `debug` or truncation for prod defaults.
- **Hooks:** `FreePromptingWorkflowHooks::before_task` uses `let _ = tx.send(...)` on full channel — same fire-and-forget pattern as elsewhere; no extra user-facing error.

### Configuration surface

- **`--recipe`:** `CoderArgs` / `DemoArgs` use clap `value_parser = ["tdd", "bugfix", "free-prompting"]` — matches resolver arms (`run.rs`).
- **Resume:** `apply_recipe_from_changeset_if_needed` loads `recipe` from `changeset.yaml` when `--recipe` is omitted and `session_dir` is set — aligns with bootstrap write in `workflow_runner` (`recipe: Some(recipe_name)`).
- **TUI:** `workflow_recipe_selection_question` + `recipe_cli_name_from_selection_label` add “Free prompting” → `free-prompting` (`backend/mod.rs`) — fourth list to keep in sync.
- **DemoArgs goals:** `DemoArgs::goal` `value_parser` still lists classic TDD/bugfix goals (plan, reproduce, …) and does **not** include **`prompting`**. For `tddy-demo` + free-prompting, users selecting an invalid goal via CLI is possible — matches the evaluation report’s “may need prompting” note.

### Security

- **No new secrets** in the reviewed code; OAuth/LiveKit env vars unchanged in scope.
- **Error strings:** Unknown recipe messages embed the user-supplied name with `{:?}` — safe for injection; may expose odd Unicode in logs.
- **Logging:** Session directory paths and agent output in logs could be sensitive in shared log aggregation — especially `plain_goal_cli_output` info-level body.

### Performance

- **Resolver:** O(1) match on recipe name; `unknown_workflow_recipe_error` allocates the “expected” string — cold path only.
- **Recipe graph:** `FreePromptingRecipe::build_graph` is tiny (echo → end); `log::info!` on every build is negligible CPU but **noisy** at scale.
- **Hooks:** Minimal work per task; event send is cheap.

---

## Risks

| Risk | Severity | Notes |
|------|----------|--------|
| **Multi-site recipe name drift** | Medium | Resolver, `approval_policy`, clap parsers, TUI labels must all agree; a partial update breaks UX or daemon YAML. |
| **Policy helper unused at runtime** | Low–Medium | `recipe_should_skip_session_document_approval` duplicates intent of `uses_primary_session_document`; drift between them is possible. |
| **Verbose agent output in logs** | Medium | `plain_goal_cli_output` info logs full output. |
| **Silent changeset write failure** | Low | `let _ = write_changeset` in presenter path. |
| **Docs / hygiene** | Process | Untracked red test output file; product docs not updated per evaluation report. |

---

## Recommendations

1. **Single source of truth for CLI names:** Prefer generating clap allowed values and/or TUI option lists from the same `&'static [&str]` as `approval_policy` / resolver (or a shared macro/const module) to eliminate drift.
2. **Wire or document `approval_policy`:** Either call `recipe_should_skip_session_document_approval` from the code paths that interpret **string** recipe names (if any), or document that **`WorkflowRecipe::uses_primary_session_document` is authoritative** and treat `approval_policy` as test-only documentation — avoid two competing definitions.
3. **Logging:** Downgrade or truncate agent output in `plain_goal_cli_output` unless debug/trace is enabled.
4. **Changeset write:** Propagate or log failures from `write_changeset` when initializing session metadata so `recipe` cannot disappear silently.
5. **Repo hygiene:** Do not add `.tddy-workflow-recipes-red-test-output.txt` to version control; complete **product docs** (`workflow-recipes.md` / changeset workflow) when merging.
6. **tddy-demo:** Extend goal allowlist or validation when recipe is `free-prompting` so `--goal prompting` (or equivalent) is consistent with the recipe.

---

## Files reviewed (production focus)

- `packages/tddy-workflow-recipes/src/recipe_resolve.rs`
- `packages/tddy-workflow-recipes/src/approval_policy.rs`
- `packages/tddy-workflow-recipes/src/free_prompting/mod.rs`
- `packages/tddy-workflow-recipes/src/free_prompting/hooks.rs`
- `packages/tddy-core/src/presenter/workflow_runner.rs` (bootstrap `recipe` in changeset)
- `packages/tddy-coder/src/run.rs` (`--recipe`, resume, validation)
- `packages/tddy-core/src/backend/mod.rs` (recipe selection question / label mapping)
