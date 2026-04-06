# Production readiness review: default `free-prompting` recipe & `/start-*` hooks

**Branch focus:** Default recipe resolution in `run.rs` / `recipe.rs`, `feature_start_slash.rs` (parse, menu labels, post-completion hook), `config.rs` recipe comment, `lib.rs` exports.  
**Workspace:** `default-free-prompt-start-slash` worktree.

---

## Summary

The default unspecified workflow recipe is consistently wired to **`free-prompting`** via `default_unspecified_workflow_recipe_cli_name()` for CLI args, changeset initialization, and resume paths. Resolution still delegates to **`tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name`** (single source of truth).

The new **`feature_start_slash`** module is **well-tested** and **exported** from `tddy-workflow-recipes`, but it is **not yet integrated** into the feature-input presenter path, TUI slash menus (`tddy_core::slash_menu_entries`), or workflow completion handling. Until that wiring lands, `/start-*` behavior exists only as a **library + acceptance-test surface**.

There is a **configuration consistency gap**: **`tdd-small`** is supported by the resolver and `approval_policy::supported_workflow_recipe_cli_names()`, but **`--recipe` clap `value_parser` lists** in `run.rs` omit it, so users cannot pass `tdd-small` on the CLI while other entry points (YAML/config, changeset, programmatic resolver) can.

**Error handling:** `write_changeset` results are **discarded** in at least two session-bootstrap paths (`let _ = write_changeset(...)`), which is a production risk if disk writes fail silently.

**Logging:** New `log::info!` calls on routine paths (default recipe name, slash parse, menu label build) may be **noisy** at default log levels. `parse_feature_start_slash_line` logs the **full raw line** at **debug**, which can include sensitive user feature text if debug logging is enabled.

---

## Strengths

- **Single source of truth** for recipe names: `recipe_resolve` + `approval_policy::supported_workflow_recipe_cli_names()` stay aligned for resolver errors and `/start-*` validation (including `tdd-small` in the supported list).
- **Explicit defaults:** `recipe_arc_for_args`, `validate_recipe_cli`, changeset `init_cs`, `clear_goal_when_not_in_recipe_goal_ids`, and `apply_agent_from_changeset_if_needed` all use the same default helper when `args.recipe` is unset.
- **Resume behavior:** `apply_recipe_from_changeset_if_needed` only fills `args.recipe` from `changeset.yaml` when CLI did not set `--recipe`, preserving CLI precedence.
- **No panics in reviewed production paths** for the new logic: resolver returns `Result`; parse returns `Option<Result<...>>`. Tests use `unwrap`/`expect` appropriately inside `#[cfg(test)]` blocks.
- **Security:** New logs record **recipe CLI names**, **suffix strings**, and **resolver error text**—not API keys or tokens. The main caveat is **debug-level logging of full feature lines** (see Risks).
- **Exports:** `lib.rs` exposes `default_unspecified_workflow_recipe_cli_name` and `resolve_workflow_recipe_from_cli_name` for integrators; `/start-*` helpers live on **`tddy-workflow-recipes`** (reasonable crate boundary).
- **Acceptance coverage:** `default_free_prompt_start_slash_acceptance.rs` ties PRD expectations to defaults, slash menu labels, parse, and post-completion hook **constants**.

---

## Risks / Gaps

| Area | Issue |
|------|--------|
| **Integration** | `feature_slash_menu_start_command_labels`, `parse_feature_start_slash_line`, and `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete` are **not referenced** from `tddy-core` presenter, `tddy-tui` (`slash_menu_entries` only adds `/recipe` + skills), or `run.rs` feature submission. **End-user `/start-*` UX is not delivered** by this branch alone. |
| **CLI vs resolver** | `#[arg(..., value_parser = ["tdd", "bugfix", "free-prompting", "grill-me"])]` **omits `tdd-small`**. Users can still use `tdd-small` via **config YAML** or **changeset** (no clap restriction), creating **asymmetric** UX and confusing `--help` text vs actual supported names. |
| **Disk I/O** | `let _ = tddy_core::changeset::write_changeset(...)` after building `init_cs` **silently ignores** write failures (e.g. plain mode bootstrap ~L1001, plan bootstrap ~L2420). Session metadata write uses `map_err`—changeset write should likely **fail the operation** or surface an error consistently. |
| **Logging volume** | `default_unspecified_workflow_recipe_cli_name()` emits **`log::info!` on every call**. If called often during startup/resume, logs may be spammy. `feature_start_slash` uses **info** for successful parses and for building menu labels (including “first label” sample). |
| **Privacy / debug** | `log::debug!("... raw line={:?}", line)` can log **entire feature descriptions** when debug is enabled—treat as **potentially sensitive** (PII, proprietary text). Prefer length-bounded or redacted debug fields for production hardening. |
| **Performance** | `feature_slash_menu_start_command_labels()` allocates a **`Vec<String>`** on each call; fine if called once at UI build, but **not ideal** on a per-keystroke path if wired naively later. |
| **Docs / comments** | `config.rs` recipe field comment lists examples; **`tdd-small`** is only implied by “etc.” Help text in tests (`cli_args.rs`) still documents four CLI recipes, not five. |

---

## Recommendations before merge

1. **Decide on `tdd-small` CLI parity:** Either add **`tdd-small`** to both `CoderArgs` and `DemoArgs` `value_parser` arrays (and update `--help` tests), or document an intentional exclusion (would be inconsistent with resolver and `/start-*` menu labels unless also removed from `supported_workflow_recipe_cli_names()`—not recommended).
2. **Wire or explicitly defer `/start-*` integration:** If the PRD requires in-app behavior, add **presenter + TUI** integration (parse on submit, merge `feature_slash_menu_start_command_labels` into slash autocomplete, reset recipe after `WorkflowComplete` via `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete`). If the branch is **library-first**, add a short **FIXME or tracking note** in `feature_start_slash.rs` or the PRD so “done” is not confused with “shipped in UI.”
3. **Handle `write_changeset` errors:** Replace `let _ = write_changeset(...)` with **`?` / `map_err` / `context`** in bootstrap paths so a full disk or permission failure does not leave the session in an inconsistent state without a clear error.
4. **Tune logging:** Downgrade routine “default is free-prompting” and per-parse **info** logs to **debug**, or log **once** per session; avoid logging full user lines except at **trace** or with **truncation**.
5. **Align help and tests:** Update `cli_args.rs` comments and assertions to include **`tdd-small`** once CLI accepts it.

---

## Completion

**Report file written:** `plans/validate-prod-ready-report.md` — **yes**
