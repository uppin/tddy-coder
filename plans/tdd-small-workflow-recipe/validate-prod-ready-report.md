# Validate prod-ready: tdd-small workflow recipe

## Scope

This report covers production-readiness review of the **`tdd-small`** workflow work under:

- `packages/tddy-workflow-recipes/src/tdd_small/` (`hooks.rs`, `recipe.rs`, `submit.rs`, `graph.rs`, `post_green_review.rs`, `red.rs`, `mod.rs`)
- Registration and policy: `recipe_resolve.rs`, `approval_policy.rs`, `lib.rs` exports

Cross-reference: `plans/tdd-small-workflow-recipe/evaluation-report.md` (risk: medium; maintainability, coupling, hygiene, product follow-up).

---

## Findings by category

### Error handling

| Finding | Severity |
|--------|----------|
| **Parse paths** use `Result` with `WorkflowError::ParseError` for red/green/post-green/refactor/update-docs outputs — consistent and fail-fast where it matters. | **low** (positive) |
| **`on_error`** logs via `log::error!` and updates changeset to `Failed`, then emits `StateChange` when an event channel is present — appropriate recovery signal. | **low** (positive) |
| **Ignored I/O results**: multiple `let _ = write_changeset(...)`, `let _ = write_red_output_file(...)`, etc. Silent persistence failures can leave the session changeset out of sync with artifacts without surfacing an error to the runner. | **medium** |
| **`after_red`** uses `read_changeset(session_dir).unwrap_or_default()` when checking session existence — a missing or corrupt changeset may be treated as empty rather than failing loudly. | **low** |
| **`before_plan`** initializes changeset with `let _ = write_changeset` on failure paths — same pattern as above. | **low** |

**Recommendations:** Prefer propagating or at least logging `warn`/`error` when `write_changeset` or critical artifact writes fail. Where `unwrap_or_default()` is intentional, a one-line comment or a dedicated helper that logs once avoids silent drift.

---

### Logging

| Finding | Severity |
|--------|----------|
| **`hooks.rs` and related modules** use `log::` (`debug`, `info`, `error`) — aligns with project guidance (no direct stdout for workflow hooks). | **low** (positive) |
| **`recipe.rs`** `plain_goal_cli_output` uses `println!` for agent output and session dir — this is the **plain CLI** trait hook, not the ratatui TUI path; same pattern as intentional non-TUI output. | **low** (informational) |
| **Verbose path logging** at `debug`/`error` (e.g. worktree failure with `repo_root`, `session_dir`) may log filesystem layout in shared logs — acceptable for ops but worth awareness in multi-tenant or sensitive trees. | **low** |

**Recommendations:** No change required for `println!` in `plain_goal_cli_output` if call sites remain CLI-only. Keep hooks on `log` only.

---

### Configuration surface

| Finding | Severity |
|--------|----------|
| Recipe is selected by CLI name **`tdd-small`** via `workflow_recipe_and_manifest_from_cli_name` and listed in `approval_policy::supported_workflow_recipe_cli_names()`. | **low** (positive) |
| **`TddSmallRecipe::default_models()`** hardcodes `opus` (plan) and `sonnet` (other goals), consistent with other shipped recipes; overrides flow through existing `resolve_model` + changeset/context `model`. | **low** |
| **Post-green contract** is documented in prompts via `tddy-tools get-schema post-green-review` and `submit` JSON parsing validates `goal == "post-green-review"`. | **low** |
| No new environment variables or secret knobs were added inside `tdd_small`. | **low** (positive) |

**Recommendations:** Ensure `tddy-tools` exposes `get-schema` / submit handling for `post-green-review` in lockstep with this recipe (see evaluation report product note).

---

### Security

| Finding | Severity |
|--------|----------|
| No API keys, tokens, or credentials in source. | **low** (positive) |
| Session **PRD content** is read for elicitation (`DocumentApproval`) and prompts — expected behavior; not written to logs at `info` in the reviewed paths (path-only debug logs). | **low** |
| **Prompts** include PRD and optional `changeset.yaml` text — user-controlled data passed to the agent; standard for this workflow layer. | **low** (informational) |

**Recommendations:** None specific to this diff beyond normal operational hygiene for log aggregation and session directory permissions.

---

### Performance

| Finding | Severity |
|--------|----------|
| **Graph build** is linear, fixed task count (seven tasks); `build_tdd_small_workflow_graph` is O(1) structure. | **low** (positive) |
| **`progress_sink` closure** may read/write `changeset.yaml` on `SessionStarted` — same class of behavior as other recipe hooks; not an obvious hot-loop issue for typical session frequency. | **low** |
| **`hooks.rs` size** (~800+ lines) increases compile and review burden but does not imply runtime hotspots. | **low** (maintainability) |

**Recommendations:** If profiling ever shows changeset churn on progress events, consider batching or debouncing (would be a cross-recipe design change).

---

## Production readiness gaps (summary)

| Gap | Severity | Notes |
|-----|----------|--------|
| Silent write failures (`let _ = ...`) | **medium** | Highest practical risk for inconsistent on-disk state vs. UI/state machine. |
| Hooks duplication vs. `TddWorkflowHooks` | **medium** | Drift risk when fixing bugs in one recipe only (per evaluation report). |
| **`after_post_green_review`** builds a minimal `EvaluateOutput` (many fields empty) for `write_evaluation_report` | **low** | Consumers expecting full evaluate-shaped reports may see sparse fields; acceptable if documented and tests cover it. |
| **`tddy-tools` schema** for `post-green-review` | **low** (product) | Confirm CLI/tooling parity with prompts. |
| **Untracked artifact** `.tdd-small-red-test-output.txt` | **low** (hygiene) | Should not ship; remove from VCS or add to `.gitignore`. |

---

## Recommendations (prioritized)

1. **[medium]** Audit `let _ = write_*` / `read_*` in `hooks.rs` and either propagate errors or log failures at `warn`/`error`.
2. **[medium]** Long term: extract shared hook helpers or tests to reduce `TddWorkflowHooks` vs `TddSmallWorkflowHooks` drift.
3. **[low]** Document or test the **shape** of `evaluation-report.md` produced after post-green when `EvaluateOutput` is partially filled.
4. **[low]** Verify **`tddy-tools submit --goal post-green-review`** and **`get-schema post-green-review`** in release tooling.
5. **[low]** Drop or ignore **`.tdd-small-red-test-output.txt`** from the repo.

---

## Conclusion

The **`tdd-small`** integration is **structurally ready** for production use: resolver/policy registration, graph topology, logging discipline on hooks, permission hints, and parse validation are coherent. The main **production risks** are **unhandled persistence errors** in hooks and **long-term maintainability** of duplicated hook logic; secondary **product** risk is **tooling/schema parity** for the merged post-green submit path.
