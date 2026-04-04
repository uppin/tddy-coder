# Clean-code analysis: `tdd_small` workflow recipe

**Scope:** `packages/tddy-workflow-recipes/src/tdd_small/` (`hooks.rs`, `recipe.rs`, `red.rs`, `submit.rs`, `post_green_review.rs`, `graph.rs`), cross-read with `tdd/hooks.rs` and `plans/tdd-small-workflow-recipe/evaluation-report.md`.

---

## 1. Naming consistency

| Area | Assessment |
|------|------------|
| Crate path vs product name | `tdd_small` (Rust module) vs recipe name `"tdd-small"` is conventional and documented in `mod.rs`. |
| Goal IDs | Kebab-case task IDs (`post-green-review`, `update-docs`) align with the rest of the workflow stack. |
| File vs responsibility | **`submit.rs`** holds JSON parsing and `PostGreenReviewOutput`, while **`post_green_review.rs`** holds prompts only. The split is logical but the name `submit` is generic—readers may expect CLI submit wiring; consider `post_green_schema.rs` or merging parser next to types under a single `post_green_review` submodule if the tree stays small. |
| Internal hooks names | `before_merged_red`, `ensure_worktree_for_merged_red`, `after_post_green_review` read clearly and distinguish the classic `tdd` red path from the merged red path. |

**Minor inconsistency:** `TddSmallRecipe::plain_goal_cli_output` uses `println!` for CLI output, unlike TUI-oriented paths—a documented contract in `WorkflowRecipe` impls would clarify when stdout is allowed (see project rules on TUI).

---

## 2. Module boundaries

**Strong:**

- **`graph.rs`** — Single responsibility: topology only; delegates `PlanTask` to `tdd::plan_task`, uses shared `BackendInvokeTask`/`EndTask`. Good boundary.
- **`red.rs`** — Recipe-owned merged-red prompts only; explicitly avoids tracking classic `crate::tdd::red` verbatim. Clear boundary.
- **`post_green_review.rs`** — User/system prompts for the merged post-green step only.
- **`submit.rs`** — Deserialize + validate `goal` field; unit test with golden JSON. Clear boundary.

**Weaker:**

- **`hooks.rs`** — Aggregates session I/O, changeset updates, event emission, worktree setup, and `RunnerHooks` implementation. It is the main integration point and **duplicates** most of `tdd/hooks.rs` (~1k lines there vs ~837 here), so the “boundary” between `tdd` and `tdd_small` is blurred at the file level: behavior is copied, not composed.
- **`recipe.rs`** — Mixes `WorkflowRecipe`, `SessionArtifactManifest`, `goal_hints`, state routing (`next_goal_for_state`), CLI printing, and a small unit test. Acceptable for a recipe struct, but state mapping includes **legacy state names** from the full TDD flow (`AcceptanceTesting`, `DemoRunning`, `Evaluating`, …) which leaks cross-recipe concerns into the small recipe’s routing table (documented inline would help).

**Coupling (from evaluation report):** `tdd_small` depends on `pub(crate)` `tdd` submodules (`green`, `refactor`, `update_docs`, `plan_task`, `session_dir_resolve`). That is intentional reuse but increases coupling surface versus a small set of stable `pub` recipe helpers.

---

## 3. Complexity (large functions / files)

| File | Lines (approx.) | Notes |
|------|-----------------|--------|
| `hooks.rs` | ~837 | Largest file; many `before_*` / `after_*` free functions plus full `RunnerHooks` impl. |
| `recipe.rs` | ~252 | Moderate; `next_goal_for_state` is dense due to compatibility branches. |
| `graph.rs` | ~95 | Small, test included. |
| `red.rs` / `submit.rs` / `post_green_review.rs` | Small | Appropriate granularity. |

**Hot spots:**

- **`before_task` / `after_task`** in `TddSmallWorkflowHooks` — Large `match` trees (similar to `TddWorkflowHooks`). Not inherently wrong, but any new goal repeats the same pattern.
- **`after_post_green_review`** — Builds a synthetic `EvaluateOutput` with many empty vectors to reuse `write_evaluation_report`. Works but couples “merged submit” to the full evaluate schema; worth isolating if `EvaluateOutput` evolves.

---

## 4. Duplication vs `TddWorkflowHooks`

The evaluation report’s **medium maintainability risk** is accurate. Substantially duplicated (often line-for-line) with `tdd/hooks.rs`:

- Helpers: `read_primary_session_document`, `read_primary_session_document_optional`, `recipe_default_models_str`, `resolve_agent_session_id`, `before_plan`, `before_green`, `before_refactor`, `before_update_docs`, `after_plan`, `after_red`, `after_green` (tdd-small omits demo-results write in `after_green`, which is an intentional delta), `after_refactor`, `after_update_docs`.
- **`RunnerHooks`**: `agent_output_sink`, `progress_sink` closure logic, elicitation for plan approval, `on_error`, and the overall structure of `before_task` / `after_task` (state events, clearing `answers` / `is_resume`).

**Intentional differences** (must stay correct when refactoring):

- Worktree: `ensure_worktree_for_merged_red` vs `ensure_worktree_for_acceptance_tests` (merged red runs before classic acceptance-tests would).
- No `acceptance-tests`, `demo`, `evaluate`, `validate` tasks; **`post-green-review`** replaces evaluate/validate for persistence.
- `after_post_green_review` maps merged JSON → evaluation report + optional `refactoring-plan.md` stub.

**Drift risk:** Any bugfix or behavior change in shared paths (e.g. session id resolution, changeset updates) may need double application until extraction.

---

## 5. SOLID (single responsibility)

- **`TddSmallWorkflowHooks`** — Violates SRP in the *structural* sense: one type owns plan, merged red, green, post-green, refactor, docs, progress/session tracking, and TUI events. This mirrors the classic `TddWorkflowHooks` design (same tradeoff).
- **Open/closed** — Adding a third recipe variant would likely copy hooks again unless shared primitives exist.
- **Dependency direction** — Good: `tdd_small` depends on `tddy_core` abstractions and `crate::parser` / `crate::writer`; bad: duplication undermines the “single place” for hook behavior.

---

## 6. Documentation (module docs / FIXME)

- **`mod.rs`** — Clear one-line description of the pipeline.
- **`graph.rs`, `red.rs`, `post_green_review.rs`** — Useful module-level docs.
- **`hooks.rs`** — Short module doc; no step-by-step of how it differs from `tdd/hooks.rs` (would reduce onboarding time).
- **`recipe.rs`** — Has `//!` on `TddSmallRecipe` / manifest; **legacy states** in `next_goal_for_state` deserve a short comment block (why `DemoRunning` / `AcceptanceTesting` appear).
- **FIXME/TODO** — None in `tdd_small/`; no deferred work called out in-code for the duplication debt (optional: a single `// NOTE:` pointing maintainers to shared-hook extraction).

**Hygiene (from evaluation report):** Untracked `.tdd-small-red-test-output.txt` should not ship—documentation or `.gitignore` alignment is a repo hygiene concern, not code quality per se.

---

## 7. Concrete improvement suggestions

1. **Extract shared hook primitives** into a dedicated module (e.g. `crate::workflow_hooks_common` or `crate::tdd::shared_hooks`) containing:
   - Document readers, `recipe_default_models_str`, `resolve_agent_session_id`, `before_plan`, `before_green`, `before_refactor`, `before_update_docs`, `after_plan`, `after_red`, `after_green` with a **policy flag or callback** for demo-results and green-complete transitions if needed.
   - Shared `progress_sink` / elicitation / `on_error` builders parameterized by recipe (`start_goal`, task id set).
   - **Goal:** one implementation, two thin adapters (`TddWorkflowHooks`, `TddSmallWorkflowHooks`) that only wire recipe-specific tasks (merged red, post-green).

2. **Split `hooks.rs` by concern** even before full deduplication: `tdd_small/hooks/plan.rs`, `merged_red.rs`, `green.rs`, `post_green.rs`, `refactor_docs.rs`, `runner.rs` re-exporting `TddSmallWorkflowHooks`. Lowers cognitive load and makes diffs smaller.

3. **`after_post_green_review`:** Add a small function in `writer` (or `submit.rs`) that writes evaluation markdown from `PostGreenReviewOutput` without constructing a full `EvaluateOutput` with empty placeholders—reduces coupling to evaluate’s struct shape.

4. **`recipe.rs`:** Document why `next_goal_for_state` maps legacy full-TDD states onto `tdd-small` goals; consider a private helper `fn normalize_small_recipe_state` if the table grows.

5. **`plain_goal_cli_output`:** If stdout is only for non-TUI CLI, add a one-line comment referencing the project rule (avoids accidental reuse in TUI contexts).

6. **Tests:** Keep golden JSON in `submit.rs`; consider an integration test that `post-green-review` round-trips through hooks + writer if not already covered by `tdd_small_acceptance.rs`.

7. **Coupling to `pub(crate) tdd`:** Long-term, promote a minimal `pub` surface for green/refactor/update_docs prompts (or move prompt modules next to shared writer/parser) so `tdd_small` does not depend on crate-private layout.

---

## Summary

The **small modules** (`graph`, `red`, `post_green_review`, `submit`) are well-scoped and readable. The main technical debt is **`hooks.rs` size and near-duplication of `tdd/hooks.rs`**, which threatens maintainability and parity fixes. Addressing that with **extracted shared helpers** and **optional file split** yields the best ROI; tightening **`after_post_green_review`**’s persistence story reduces coupling to the full evaluate model.
