# Bugfix analyze — clean code review

## Executive summary

The bugfix **analyze** work is **structurally sound**: responsibilities are split across `bugfix/mod.rs` (graph and recipe policy), `bugfix/hooks.rs` (hook orchestration), `bugfix/analyze.rs` (prompts + persistence), `parser.rs` (JSON shape), and `stub.rs` (deterministic test JSON). Naming aligns with the rest of the workflow (goal ids, `Changeset` fields, log prefixes).

The main **maintainability** concerns are: (1) **`summary` is parsed but never persisted or merged** into the reproduce step**, so `AnalyzeOutput.summary` is effectively unused in production paths** (`analyze.rs`); (2) **`before_task("analyze")` in hooks is a long, nested block** that duplicates the *seed changeset + set state* pattern already present in TDD’s `before_plan` (`tdd/hooks.rs`); (3) **`next_goal_for_state`’s catch-all default** to `analyze` is powerful and should stay explicit in code comments or a small helper so future states do not silently resume at the wrong goal.

Complexity is **moderate** and appropriate for a demo recipe; **SOLID** boundaries are respected (recipe vs hooks vs pure parse/persist). Documentation is **good at module and public API level** but slightly below the **TDD hooks** file in terms of extracted helpers and sectional structure.

---

## What is good

- **Clear module boundaries**: `BugfixRecipe` does not embed JSON parsing; `apply_analyze_submit_to_changeset` (`bugfix/analyze.rs`) owns the merge into `changeset.yaml` and state transition to `Reproducing`.
- **Consistent naming**: `"analyze"` / `"reproduce"` strings match graph tasks, `GoalId`, parser validation, and `StubBackend::analyze_response` (`packages/tddy-core/src/backend/stub.rs`).
- **Parser pattern** (`packages/tddy-workflow-recipes/src/parser.rs`): `StructuredAnalyze` + `parse_analyze_response` mirrors other goals (private serde struct, trim/validate, `ParseError::Malformed`) — easy for readers familiar with `PlanningOutput` / other parsers in the same file.
- **Observable behavior**: Structured logging with prefixes (`[bugfix hooks]`, `[bugfix analyze]`) matches project style and aids debugging without TUI stdout noise.
- **Stub parity**: `analyze_response` submits minimal valid JSON including `branch_suggestion`, `worktree_suggestion`, and `name` (`stub.rs` ~226–232), keeping integration tests deterministic.
- **Tests colocated**: Recipe unit tests in `bugfix/mod.rs` assert graph order and plugin contract; persistence has a dedicated test file per the evaluation report.

---

## Issues

| Area | Location | Notes |
|------|-----------|--------|
| **Unused parsed field / PRD gap** | `packages/tddy-workflow-recipes/src/bugfix/analyze.rs` — `apply_analyze_submit_to_changeset` uses `branch_suggestion`, `worktree_suggestion`, `name` only; **`summary` from `AnalyzeOutput` is never read** after `parse_analyze_response` (see `parser.rs` ~152–157 for struct). Aligns with evaluation-report “optional summary not merged into reproduce prompt.” |
| **Duplication vs TDD hooks** | `packages/tddy-workflow-recipes/src/bugfix/hooks.rs` ~48–72 vs `packages/tddy-workflow-recipes/src/tdd/hooks.rs` ~150–173 (`before_plan`) | Same conceptual steps: ensure changeset exists with `initial_prompt` / `repo_path` / recipe tag, then **`update_state` + write**. Bugfix adds `recipe: Some("bugfix")` and branches on `read_changeset().is_err()` only for seeding. |
| **`before_task` complexity** | `packages/tddy-workflow-recipes/src/bugfix/hooks.rs` ~38–111 | Single match arm for `"analyze"` combines: seed changeset, set `Analyzing`, answer handoff, events. **High cyclomatic density** compared to TDD, which extracts `before_plan`, `ensure_worktree_*`, etc. |
| **Broad default in recipe policy** | `packages/tddy-workflow-recipes/src/bugfix/mod.rs` ~94–100 | `next_goal_for_state`: everything except `Failed` and `Reproducing` maps to **`analyze`**. Correct for “resume / legacy” but easy to misread; evaluation report already flags this as a workflow nuance. |
| **Generic public names** | `packages/tddy-workflow-recipes/src/bugfix/analyze.rs` ~14–15, ~40 | `system_prompt` / `reproduce_system_prompt` are clear in context but **generic for ripgrep** across the crate; not wrong, just a minor discoverability tradeoff vs `bugfix_analyze_system_prompt`-style names. |
| **Stub JSON completeness (optional)** | `packages/tddy-core/src/backend/stub.rs` ~226–232 | Valid minimal payload; **`summary` omitted** (optional in schema). Harmless for tests but **does not exercise** summary in stub-driven flows. |
| **Documentation depth vs TDD** | `packages/tddy-workflow-recipes/src/bugfix/hooks.rs` (file header ~2 lines) vs `tdd/hooks.rs` (long file doc + many `fn before_*`) | Bugfix hooks are **simpler** but also **less guided** for a new contributor expecting the same “extracted phase functions” narrative. |

---

## Refactoring suggestions

1. **Close the `summary` loop (product + code clarity)**  
   Either persist `summary` on `Changeset` if the model supports it, or prepend it to context/`prompt` in `before_task("reproduce")` when present. Until then, consider **`#[allow(dead_code)]` is wrong** — better: **use the field** or **remove it from `AnalyzeOutput`** until the feature ships (avoid dead API surface).

2. **Extract bugfix `before_analyze` (or `seed_bugfix_changeset`)**  
   Move the block in `hooks.rs` ~45–81 into **one or two private functions** in `hooks.rs` or `analyze.rs` (e.g. `seed_changeset_for_analyze(session_dir, context)` and `set_changeset_state_analyzing(session_dir)`), mirroring `before_plan` in TDD. Reduces nesting and makes `before_task` read as a dispatch table only.

3. **Shared helper for “init changeset from context” (only if duplication grows)**  
   If more recipes need the same `feature_input` + `output_dir` → `Changeset` seed, a **single function in `tddy_core::changeset`** (or `tddy_workflow_recipes` util) would respect **DRY** without coupling bugfix to TDD. **Do not introduce** this until a third copy appears, unless the team prefers centralizing now — tradeoff is API surface vs duplication.

4. **Document `next_goal_for_state` contract**  
   Add a **short comment** on `bugfix/mod.rs` ~94–100 explaining which workflow states are first-class vs legacy and why the default is `analyze` (file:line target for reviewers).

5. **Stub: optional `summary` field**  
   Add `"summary":"…"` to `analyze_response` JSON in `stub.rs` ~228–231 so integration tests can assert end-to-end behavior when summary merging is implemented.

6. **Naming (optional polish)**  
   Rename exports to `analyze_system_prompt` / `reproduce_system_prompt` **only if** the team wants consistency with other recipe modules; update `hooks.rs` imports accordingly. Low priority if call sites stay within `bugfix`.

---

## Confirmation

**File written:** `/var/tddy/Code/tddy-coder/.worktrees/bugfix-analyze/plans/bugfix-analyze/analyze-clean-code-report.md`
