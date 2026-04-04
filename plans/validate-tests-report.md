# Validate-tests report: TDD interview branch

**Date:** 2026-04-04

**Command run:** `./test` from repository root  
`/var/tddy/Code/tddy-coder/.worktrees/tdd-interview`

**What `./test` does (for traceability):** enters nix dev shell (`nix develop --profile ./.nix-profile`), builds `tddy-coder`, `tddy-tools`, `tddy-livekit`, and `tddy-acp-stub` (`--examples --bins`), then `cargo test --workspace -- --test-threads=1` with output tee’d to `.verify-result.txt`.

**Overall result:** **PASS** — exit code `0`. Wall-clock **~229 s** (~3.8 min). No fallback was required (`./dev cargo test --workspace` not used).

**Aggregate counts (from parsing `test result:` lines in the run log):**

- **~1003** individual `passed` assertions summed across all test harness result lines (workspace-wide; each binary/harness reports its own totals).
- **144** separate `test result: ok.` summaries (library tests, integration tests, doc-tests, etc.).
- **0** failed tests in any harness line (`N failed` was always `0`).

---

## Failing tests

**None.** No `FAILED`, panics, or non-zero `failed` counts appeared in the captured output.

---

## Passing highlights relevant to interview

| Area | Notes |
|------|--------|
| **Workflow recipes — acceptance** | `tdd_recipe_start_goal_is_interview`, `tdd_interview_goal_hints_and_submit_policy`, `tdd_interview_handoff_populates_plan_context` — `packages/tddy-workflow-recipes/tests/tdd_interview_acceptance.rs` |
| **Relay helpers — unit** | `persist_interview_handoff_writes_relay_file`, `apply_staged_interview_handoff_sets_answers_on_context` — `packages/tddy-workflow-recipes/tests/tdd_interview_handoff_unit.rs` |
| **Graph ordering** | `tdd_graph_interview_precedes_plan`, `tdd_full_graph_interview_precedes_plan` — `packages/tddy-integration-tests/tests/workflow_graph.rs` |
| **CLI default goal** | TDD recipe starts at `interview` — `packages/tddy-coder/tests/cli_recipe.rs` |
| **Full workflow / session** | Mock stack includes interview turn without submit — `packages/tddy-integration-tests/tests/single_session_lifecycle_acceptance.rs` |
| **E2E / gRPC / PTY** | Workflow graphs and presenters updated for interview-before-plan — e.g. `packages/tddy-e2e/tests/grpc_full_workflow.rs`, `grpc_terminal_rpc.rs`, `grpc_concurrent_resize.rs`, `pty_full_workflow.rs`, `grpc_reconnect_acceptance.rs` |
| **Integration — start goal** | `full_workflow_integration.rs` expectations for `GoalId::new("interview")` — `packages/tddy-integration-tests/tests/full_workflow_integration.rs` |
| **Submit policy matrix** | `TddRecipe` still requires submit for `plan` — `packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs` (`tdd_plan_goal_still_requires_tddy_tools_submit`) |

---

## Coverage gaps / recommendations

1. **`BackendInvokeTask` without `tddy-tools` submit for `interview`**  
   `packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs` exercises **GrillMe** and **FreePrompting** with an output-only backend, and asserts **plan** still requires submit. There is **no parallel async test** that builds `BackendInvokeTask::from_recipe("interview", …, &TddRecipe, …)` and proves completion when the backend never supplies submit (the policy is covered in `tdd_interview_goal_hints_and_submit_policy`, but not the same integration shape as grill/free_prompting).

2. **Relay edge cases (`apply_staged_interview_handoff_to_plan_context`)**  
   Implementation treats **missing** file and **empty** (whitespace-only) file as success with no `answers` set (`packages/tddy-workflow-recipes/src/tdd/interview.rs`). Current unit tests cover happy path and non-empty content; **no test** asserts the deliberate no-op when the file is absent or trimmed-empty (guards regressions if someone later turns those into errors).

3. **Resume / restart paths**  
   E2E and integration tests touch full flows; **explicit** acceptance tests for “resume session with partial interview” or “restart at `plan` with stale handoff” are thin compared to the main happy path. Worth a focused scenario if session/store semantics keep evolving.

4. **Hooks-level contract**  
   `before_interview` / `after_interview` in `packages/tddy-workflow-recipes/src/tdd/hooks.rs` are exercised indirectly via flow tests; a **narrow** test that runs only interview hooks and asserts relay file + context fields could shorten debug time when hooks change.

5. **Daemon / ACP / stub parity**  
   If production paths differ from `MockBackend` for submit detection, add a **single** contract test aligned with existing `green_submit_remediation`-style tests but scoped to **interview** output-only completion (only if product requires it).

---

## Key test file references (`packages/`)

- `tddy-workflow-recipes/tests/tdd_interview_acceptance.rs` — recipe metadata, hints, handoff → plan context  
- `tddy-workflow-recipes/tests/tdd_interview_handoff_unit.rs` — relay persistence and merge  
- `tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs` — output-only invoke pattern (extend for TDD interview)  
- `tddy-integration-tests/tests/workflow_graph.rs` — interview before plan, handoff version  
- `tddy-integration-tests/tests/single_session_lifecycle_acceptance.rs` — full TDD graph with interview mock turn  
- `tddy-integration-tests/tests/full_workflow_integration.rs` — start goal interview  
- `tddy-coder/tests/cli_recipe.rs` — CLI default `interview`  
- `tddy-e2e/tests/grpc_full_workflow.rs`, `grpc_terminal_rpc.rs`, `pty_full_workflow.rs`, `grpc_reconnect_acceptance.rs`, `grpc_concurrent_resize.rs` — end-to-end workflow and UI markers  

---

*Generated by validate-tests subagent run.*
