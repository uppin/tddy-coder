# Changeset: plain-mode-free-prompting-completion — plain CLI completes single-turn free-prompting instead of crashing

**Date:** 2026-07-01
**Branch:** `regular-marimba`
**Packages:** `tddy-core`, `tddy-coder`
**Feature PRD:** [docs/ft/coder/1-WIP/PRD-2026-07-01-plain-mode-free-prompting-completion.md](../../ft/coder/1-WIP/PRD-2026-07-01-plain-mode-free-prompting-completion.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Persist `context["output"]` in `BackendInvokeTask`'s no-submit `Continue` branch
      (`packages/tddy-core/src/workflow/backend_invoke_task.rs`)
- [x] Add `waiting_for_input_has_pending_questions` helper (`packages/tddy-coder/src/run.rs`)
- [x] Fix `run_full_workflow_plain`'s `WaitingForInput` arm to complete when no pending questions
- [x] Fix `run_goal_plain`'s `WaitingForInput` arm to complete when no pending questions

## State A (Current)

Two compounding gaps (see PRD Background for full detail):

1. `BackendInvokeTask::run`'s no-submit/no-clarification `Continue` branch
   (`packages/tddy-core/src/workflow/backend_invoke_task.rs:346-351`) never calls
   `context.set_sync("output", ...)`, unlike the submit-success branch just above it. The response
   text only ever reaches `TaskResult.response`, which `tddy-graph`'s `FlowRunner` discards.
2. `run_full_workflow_plain` and `run_goal_plain` (`packages/tddy-coder/src/run.rs`) treat every
   `ExecutionStatus::WaitingForInput` as an explicit clarification request:

   ```rust
   let questions: Vec<tddy_core::ClarificationQuestion> = session
       .context
       .get_sync("pending_questions")
       .ok_or_else(|| anyhow::anyhow!("no pending questions"))?;
   ```

`FreePromptingRecipe`'s single, no-successor `prompting` task causes `tddy-graph`'s `FlowRunner` to
synthesize this same `WaitingForInput` status when a backend simply finishes a turn without
submitting or asking a question. Result: any plain-mode, single-shot `--recipe free-prompting`
invocation crashes with `Error: no pending questions` on its first turn, confirmed with both
`--agent stub` and `--agent fastcontext`.

## State B (Target)

`BackendInvokeTask`'s no-submit `Continue` branch persists the response into `context["output"]`.
Both plain-mode entry points distinguish "genuine clarification pending" from "turn complete, no
follow-up task" using that same `pending_questions` context value they already read. When absent
(or empty), the plain CLI treats the status like `Completed`: print `context["output"]`, print
session info, and exit 0.

## Delta

### New
- `context.set_sync("output", response.output.clone())` call in `BackendInvokeTask`'s no-submit
  `Continue` branch.
- `waiting_for_input_has_pending_questions(&Context) -> bool` helper in `run.rs`, unit-tested for
  absent / non-empty / empty `pending_questions`.
- New test case in `packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs`
  asserting `context["output"]` is populated after the no-submit `Continue` path.
- `packages/tddy-coder/tests/free_prompting_plain_mode_acceptance.rs` — CLI-level acceptance tests.

### Modified
- `run_full_workflow_plain`'s `WaitingForInput` arm: branch on the new helper before falling back
  to the existing "read answers from stdin" behavior. When no pending questions are present,
  prints via `print_goal_output` (using `result.current_task_id` as the goal id, since this path
  has no `goal` string in scope), then the session dir and `print_session_info_on_exit`, then
  returns `Ok(())`.
- `run_goal_plain`'s `WaitingForInput` arm: same branch, mirroring the existing `Completed |
  Paused` arm's output/session-dir resolution and print calls in the same function.
- `FreePromptingRecipe::plain_goal_cli_output`
  (`packages/tddy-workflow-recipes/src/free_prompting/mod.rs`): changed from `log::info!`-only
  (invisible in the `tddy-coder` binary, which never initializes a `log` backend) to `println!`,
  so the backend's response text is actually visible to plain-mode CLI users on stdout.

### Removed
- None.

## Milestones

### Milestone 1: Red — failing acceptance + unit tests (this changeset)
- [ ] CLI acceptance test reproduces the crash and fails for the right reason
- [ ] Unit tests for the new predicate fail to compile (function doesn't exist yet)
- [ ] `tddy-workflow-recipes` integration test asserting `context["output"]` fails (key never set)

### Milestone 2: Green — implement the fix
- [x] Persist `context["output"]` in `BackendInvokeTask`
- [x] Add the predicate helper
- [x] Wire it into both plain-mode `WaitingForInput` arms
- [x] All new tests pass; full `tddy-coder` + `tddy-workflow-recipes` suites still green

## Testing Strategy

### Acceptance Tests
- [x] `free_prompting_plain_mode_completes_a_single_turn_and_prints_the_response`
- [x] `plan_goal_clarification_still_prompts_for_answers_and_completes_in_plain_mode`

### Unit / Integration Tests
- [x] `free_prompting_backend_invoke_persists_output_into_context_for_plain_mode_completion`
      (`packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs`)
- [x] `waiting_for_input_has_pending_questions` — absent / non-empty / empty cases
      (`packages/tddy-coder/src/run.rs`)

### Test Level Decisions

| Aspect | Level | Rationale |
|--------|-------|-----------|
| Plain-mode `WaitingForInput` completion behavior, end-to-end | Acceptance (CLI, `assert_cmd`) | Only observable through real CLI dispatch (`run_full_workflow_plain`/`run_goal_plain` are private); `--agent stub` keeps it deterministic and network-free |
| `context["output"]` persistence for the no-submit `Continue` path | Integration (`tddy-workflow-recipes`, in-process `FlowRunner`) | `BackendInvokeTask` needs a real `WorkflowRecipe`, which `tddy-core` cannot depend on; mirrors the existing sibling test in the same file |
| `pending_questions` presence/emptiness predicate | Unit | Pure function, no I/O — isolates the exact distinguishing signal from the rest of the CLI plumbing |

## Technical Debt

- `run_plan_bootstrap_in_session_dir` / `run_plan_refinement` still duplicate the identical
  `WaitingForInput` → `pending_questions` pattern for the plan-approval flow; left untouched here
  since that path always sets `pending_questions` and is out of this changeset's scope.
- `--goal prompting` cannot be passed via the CLI today (clap's `value_parser` for `CoderArgs::goal`
  doesn't include it), so `run_goal_plain`'s fix for the "Continue, no successor, no questions" arm
  has no CLI-reachable acceptance coverage of its own — only the unit-level predicate and the
  `run_full_workflow_plain` acceptance test exercise that logic. Tracked here rather than widening
  this changeset's scope to add a new `--goal` value.

## Validation Results

### validate-changes (2026-07-01)

**Critical (0) · Warning (0) · Info (2)**

Scope note: the working tree also carries an unrelated, already-completed piece of work from
earlier the same session — `--fastcontext-model` CLI/config plumbing
(`packages/tddy-coder/src/config.rs`, `run.rs`'s `create_backend`, `packages/tddy-discovery/src/backend.rs`,
`packages/tddy-discovery/examples/discover.rs`) — which has no PRD/changeset of its own (it was
implemented directly from an approved plan-mode plan, not via `/plan-red`). It was re-validated here
alongside this changeset since both are in the same uncommitted diff; no issues found.

#### File-level notes

| File | Status | Notes |
|------|--------|-------|
| `packages/tddy-core/src/workflow/backend_invoke_task.rs` | ✅ | Single-line, additive; mirrors the existing submit-success branch's `context.set_sync("output", ...)` call exactly |
| `packages/tddy-coder/src/run.rs` (`waiting_for_input_has_pending_questions`) | ✅ | Pure function, no I/O, no shared mutable state |
| `packages/tddy-coder/src/run.rs` (`run_full_workflow_plain`, `run_goal_plain`) | ✅ | New `println!`/`plain_goal_cli_output` calls verified confined to plain-mode-only functions (`run_goal_plain`, `run_full_workflow_plain`) — never reachable from `run_full_workflow_tui` or any ratatui rendering path; traced all 4 call sites of `print_goal_output` and the sole production call site of `plain_goal_cli_output` to confirm |
| `packages/tddy-workflow-recipes/src/free_prompting/mod.rs` | ✅ | `log::info!` → `println!` matches the convention `BugfixRecipe::plain_goal_cli_output` already uses; independently confirmed `log::info!` was invisible in plain-mode output (logs redirect to a per-session `debug.log` file, not stderr) |
| `packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs` | ✅ | New test only; existing 8 tests unmodified |
| `packages/tddy-coder/tests/free_prompting_plain_mode_acceptance.rs` | ℹ️ | New file; see `analyze-clean-code`/`validate-tests` passes for style notes |
| `packages/tddy-discovery/examples/discover.rs` | ℹ️ | Doc comment still says "delete after manual verification", but the user explicitly asked to keep this file — comment is now stale/contradictory; flagged for the refactor pass |

No unwrap/expect without justification in production code, no unsafe blocks, no hardcoded secrets,
no test-env-only production branches, no fallbacks added without consent, no breaking API changes
(`create_backend` and `BackendInvokeTask::new` are both private; `FastContextBackend::new`'s
signature is unchanged). `cargo build -p tddy-coder -p tddy-core -p tddy-workflow-recipes
-p tddy-discovery` succeeds cleanly.
