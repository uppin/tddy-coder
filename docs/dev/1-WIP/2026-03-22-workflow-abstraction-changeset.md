# Changeset: Workflow Abstraction Layer

**Date**: 2026-03-22  
**PRD**: [docs/ft/coder/1-WIP/PRD-2026-03-22-workflow-abstraction.md](../../ft/coder/1-WIP/PRD-2026-03-22-workflow-abstraction.md)  
**Plan**: `.cursor/plans/workflow_abstraction_layer_1f4095c3.plan.md` (do not edit)

## Plan mode summary

Replace compile-time `Goal` enum and hard-coded TDD graph with:

- **`GoalId`**, **`WorkflowState`** — string-backed newtypes (serde-transparent).
- **`GoalHints`** + **`PermissionHint`** — backend configuration from recipe.
- **`WorkflowRecipe`** trait — graph, hooks, state machine, permissions, artifacts.
- New **`tddy-workflow-recipes`** — `TddRecipe` (and future `BugfixRecipe` stub).

## Affected packages

- `tddy-core` — abstraction types, recipe trait, engine parameterized by recipe; remove TDD-specific modules from core.
- `tddy-workflow-recipes` — new crate: TDD graph, hooks, prompts, parsers, permissions, state machine, stub helpers.
- `tddy-coder`, `tddy-service`, `tddy-daemon`, `tddy-demo` — depend on recipes; wire default `TddRecipe`.

## Milestones

1. Core types + `WorkflowRecipe` in `tddy-core`.
2. `tddy-workflow-recipes` with `TddRecipe` and migrated TDD code.
3. `WorkflowEngine` + backends + `InvokeRequest` use `GoalId` + `GoalHints`.
4. CLI, presenter, daemon use recipe for goals and status.
5. Remove TDD remnants from core; add `BugfixRecipe` stub for OCP proof.

## Acceptance

- [x] `./test` passes (2026-03-22: exit 0; ~644 test result lines summed across crates — see `.verify-result.txt`).
- [x] `tddy-core` has no `Goal` enum, no `tdd_graph` / `tdd_hooks` / TDD-only permission lists.
- [x] Second workflow compiles without changing core types (`BugfixRecipe` stub in `tddy-workflow-recipes`).

## PR-wrap validation (2026-03-22)

| Check | Result |
|--------|--------|
| `cargo fmt --all` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `./test` | PASS |
| Cursor `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code` | Not run in this session — use before merge if your workflow requires them |

**Status:** Ready for review (toolchain green).

### Clippy fixes during wrap

- `WorkflowState`: `Default` via `#[derive(Default)]` (`packages/tddy-core/src/workflow/ids.rs`).
- Integration tests: import `TddWorkflowHooks` from `tddy_workflow_recipes` (avoid unused `pub use` in shared `common` module); remove stale unused imports; drop redundant `session_dir` rebinding.
