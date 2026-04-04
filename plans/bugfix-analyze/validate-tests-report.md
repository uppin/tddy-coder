# Validate Tests Report — Bugfix Analyze

## Executive summary

The full Rust workspace test suite was run with a single test thread. **All tests passed** (exit status **0**); **0** failures were observed. Existing coverage for the bugfix analyze feature is **solid for happy paths** (recipe graph, CLI resolver, schema registration, stub submit JSON, hook persistence). **Gaps** are concentrated in **direct unit tests for `parse_analyze_response`**, **negative paths** for `apply_analyze_submit_to_changeset`, **stronger assertions** on persisted changeset state and optional fields, and **documentation/comments** that still describe the old “start at reproduce” behavior in one CLI test.

## Test run command and outcome

| Field | Value |
|--------|--------|
| **Command** | `./dev cargo test --workspace -- --test-threads=1` |
| **Working directory** | Repository root (`/var/tddy/Code/tddy-coder/.worktrees/bugfix-analyze`) |
| **Outcome** | **PASS** |
| **Exit status** | **0** |
| **Failures** | **0** |

The run completed in approximately **9 minutes** of wall time (including workspace compile). No `FAILED`, `failures:`, or panic lines appeared in the captured output.

## Per-crate / notable test summary

| Area | Package / location | Role for bugfix analyze |
|------|---------------------|-------------------------|
| Recipe graph & plugin | `tddy-workflow-recipes` unit tests in `bugfix/mod.rs` | `bugfix_graph_orders_*`, `bugfix_recipe_is_valid_plugin`, `bugfix_reproduce_does_not_require_tddy_tools_submit` |
| Persistence hook | `tddy-workflow-recipes/tests/bugfix_analyze_persistence.rs` | `after_task("analyze")` persists branch/worktree via hooks |
| CLI recipe | `tddy-coder/tests/cli_recipe.rs` | Resolver + `start_goal` == `analyze` |
| Schema registry | `tddy-tools/tests/schema_validation_tests.rs` | `analyze_goal_schema_embedded` |
| CLI integration (tools) | `tddy-tools/tests/cli_integration.rs` | `analyze` in registered goals / URN parity |
| Stub backend + recipe | `tddy-integration-tests/tests/workflow_graph.rs` | `stub_or_mock_backend_analyze_submit_valid` |
| Core stub implementation | `tddy-core` — covered indirectly | `StubBackend::analyze_response` exercised by integration test above |

Other packages ran their usual suites; nothing in the run indicated regressions tied to analyze.

## Missing coverage / risks

1. **`parse_analyze_response` (`parser.rs`)**  
   There are **no dedicated unit tests** for malformed JSON, wrong `goal`, empty `branch_suggestion` / `worktree_suggestion`, or optional `summary` / `name` handling. Behavior is only exercised indirectly through the persistence acceptance test (valid JSON only). **Risk:** regressions in validation messages or trimming logic may go unnoticed until integration.

2. **`apply_analyze_submit_to_changeset` (`bugfix/analyze.rs`)**  
   **Empty** task response and **parse failure** paths are not covered by a direct test. The hook test uses a valid payload only.

3. **`bugfix_analyze_persistence` assertions**  
   The test checks that branch and worktree are **Some** but does not assert **exact values**, **`Changeset` workflow state** transition to **Reproducing**, or **`name`** content. Optional **`summary`** in JSON is not asserted (aligns with known product gap: summary not merged downstream).

4. **`goal_requires_tddy_tools_submit`**  
   Unit tests assert **reproduce** does **not** require submit; there is **no explicit** assertion that **analyze** **does** require submit (likely true by inspection but not locked by test).

5. **Schema test depth (`analyze_goal_schema_embedded`)**  
   Ensures non-empty embedded schema and loose shape hints; it does **not** validate a document instance against the schema (unlike richer tests for other goals in the same file).

6. **Comment / doc drift**  
   `packages/tddy-coder/tests/cli_integration.rs` still documents bugfix resume behavior with “start goal `reproduce`” in a doc comment — **stale** relative to analyze-first; risks confusing future maintainers (not a runtime bug).

7. **End-to-end CLI**  
   `cli_recipe` covers resolver/start goal; broader **`tddy-coder` CLI** paths for bugfix are not specifically extended for analyze-first beyond existing stub/resume tests (see evaluation report).

## Recommendations

1. Add **`parser` unit tests** for `parse_analyze_response`: success with optional `name`/`summary`, and failures for wrong goal, missing fields, and empty strings after trim.

2. Add **one or two tests** for `apply_analyze_submit_to_changeset`: empty response returns `Err`; invalid JSON returns `Err` with stable messaging.

3. **Strengthen** `bugfix_analyze_persists_branch_and_worktree`: assert expected strings, `WorkflowState::Reproducing`, and optionally `name` when present.

4. Add **`assert!(r.goal_requires_tddy_tools_submit(&GoalId::new("analyze"))`** next to the existing reproduce guard in `bugfix/mod.rs` tests.

5. Optionally extend **schema** test to **validate** a minimal conforming analyze instance (if consistent with how other goals are tested in `schema_validation_tests.rs`).

6. **Fix** the stale **doc comment** in `cli_integration.rs` (bugfix resume test) to say **analyze** (or “current start goal”) instead of reproduce.

7. Keep **`.red-phase-submit.json`** out of version control per evaluation-report hygiene notes (not a test gap, but affects clean CI/workspace signals).

---

*Generated as part of validate-tests subagent work for plans/bugfix-analyze (see `evaluation-report.md`).*
