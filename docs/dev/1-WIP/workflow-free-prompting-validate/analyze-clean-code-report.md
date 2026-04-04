# Clean-code analysis: workflow free-prompting + approval policy

## Summary

The new work is **coherent and appropriately layered**: `FreePromptingRecipe` lives under `free_prompting/`, resolution and user-facing errors are centralized in `recipe_resolve`, and session-document approval policy is isolated in `approval_policy`. Naming is mostly consistent (`free-prompting` CLI vs `FreePromptingRecipe` type vs `free_prompting` graph id). The main maintainability risk is **enumerated recipe names duplicated** across resolver match arms, policy allowlists, CLI `value_parser` arrays, and TUI label→CLI maps—tests reduce drift but do not eliminate it. Documentation is adequate at module level; a few public helpers could use one-line docs.

## Strengths

- **Clear separation of concerns:** `recipe_resolve` owns name→`WorkflowRecipe` mapping and error text; `approval_policy` owns F2/F3 policy predicates without embedding resolver logic; `FreePromptingRecipe` implements `WorkflowRecipe` + `SessionArtifactManifest` like other recipes.
- **Trait-oriented design:** Policy gates use `WorkflowRecipe::uses_primary_session_document` and recipe metadata rather than hard-coding recipe strings in core (beyond TUI/CLI wiring).
- **Targeted tests:** `workflow_recipe_acceptance.rs` and `recipe_policy_red.rs` assert resolver + policy invariants (including `free-prompting` in error strings and supported names).
- **`workflow_runner` bootstrap:** Writing `recipe` into the initial changeset in `run_start_goal_without_output_dir` is a single, explicit place for session parity (see `packages/tddy-core/src/presenter/workflow_runner.rs`).

## Issues (with severity)

| Severity | Issue |
|----------|--------|
| **Medium** | **Duplicate recipe enumerations:** `approval_policy::supported_workflow_recipe_cli_names`, `workflow_recipe_and_manifest_from_cli_name` match arms, `tddy-coder`/`DemoArgs` `value_parser = [...]`, and `backend::recipe_cli_name_from_selection_label` + `workflow_recipe_selection_question` options must all be updated together when adding a recipe. Drift causes wrong CLI validation, wrong error messages, or broken TUI selection. |
| **Low** | **`approval_policy` debug log** duplicates the literal slice in the log message (`&["tdd", "bugfix", "free-prompting"]`) instead of logging the returned slice—minor duplication if the table changes. |
| **Low** | **`FreePromptingRecipe::next_goal_for_state`:** The `_ => Some(GoalId::new("prompting"))` arm maps any non-`Failed` unknown state back to `prompting`; intentional for resilience but worth a short comment if unexpected states are possible. |
| **Low** | **Product/docs gap** (from evaluation-report): `packages/tddy-workflow-recipes` / workflow-recipes feature docs not updated in this changeset; public behavior is discoverable from code and tests only. |

## Refactoring suggestions (concrete, file-scoped)

1. **`packages/tddy-workflow-recipes/src/recipe_resolve.rs` + `approval_policy.rs`**  
   - Introduce a **single const table** of supported CLI names (e.g. `pub const SUPPORTED_WORKFLOW_RECIPE_CLI_NAMES: &[&str] = &[...]`) in one module, or generate `supported_workflow_recipe_cli_names()` from the same source as the resolver.  
   - Alternatively, keep the match as the source of truth and **derive** the allowlist by calling a private registry function that returns `&[&str]` built once from the same `match` (harder without macros).  
   - Minimum incremental fix: add a **`#[test]`** that parses `supported_workflow_recipe_cli_names()` and asserts each name `workflow_recipe_and_manifest_from_cli_name(name).is_ok()`—catches resolver/policy mismatch immediately.

2. **`packages/tddy-coder/src/run.rs` (CoderArgs / DemoArgs)**  
   - Replace duplicated `value_parser = ["tdd", "bugfix", "free-prompting"]` with a **shared helper** or constant re-exported from `tddy_workflow_recipes` (e.g. clap `PossibleValuesParser` built from `supported_workflow_recipe_cli_names()`) so CLI and resolver cannot diverge.

3. **`packages/tddy-core/src/backend/mod.rs`**  
   - **`workflow_recipe_selection_question`** and **`recipe_cli_name_from_selection_label`** duplicate human labels vs CLI names. Consider a small **struct table** `(label, cli_name, description)` iterated to build `ClarificationQuestion` and a single match for label→CLI, reducing the chance of adding a TUI option without a resolver entry.

4. **`packages/tddy-workflow-recipes/src/approval_policy.rs`**  
   - Add **`///`** on `recipe_should_skip_session_document_approval` clarifying that it is keyed by **CLI name** (trimmed), and that it must stay aligned with `WorkflowRecipe::name()` for recipes that opt out of approval.  
   - Fix the **debug log** to log the actual returned slice (e.g. `supported_workflow_recipe_cli_names()`) to avoid three copies of the same literals.

5. **`packages/tddy-workflow-recipes/src/free_prompting/mod.rs`**  
   - Optional: one-line **`///`** on `FreePromptingRecipe` describing F1 scope (single Prompting loop, no TDD pipeline).  
   - If `_ => Some(prompting)` in `next_goal_for_state` is deliberate, add a **one-line comment** referencing expected workflow states.

## Optional: `grpc_terminal_rpc` `utf8_preview` pattern

In `packages/tddy-e2e/tests/grpc_terminal_rpc.rs`, `utf8_preview` uses **`s.chars().take(max_chars)`** so log/assertion snippets truncate on **Unicode scalar boundaries**, avoiding split codepoints when previewing large terminal output. This pairs with `String::from_utf8_lossy` elsewhere for byte-oriented paths. Good pattern for **debuggable e2e output** without corrupting UTF-8 in previews.
