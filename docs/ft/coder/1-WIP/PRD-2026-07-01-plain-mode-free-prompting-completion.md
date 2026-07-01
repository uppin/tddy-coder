# PRD: Plain-mode completion for single-turn free-prompting

**Created:** 2026-07-01
**Product Area:** coder
**Status:** WIP

## Summary

`tddy-coder`'s plain (non-TUI) CLI mode crashes with `Error: no pending questions` when running
the `free-prompting` recipe with a backend that neither calls `tddy-tools submit` nor returns
clarification questions — including the `stub` backend and the `fastcontext` (Discovery / Ollama)
backend. This PRD scopes a fix so a single-shot, non-interactive `free-prompting` invocation
completes and prints the backend's output instead of crashing.

## Background

`FreePromptingRecipe` (`packages/tddy-workflow-recipes/src/free_prompting/mod.rs`) builds a graph
with exactly one task, `prompting`, and **no outgoing edge** — by design, so that interactive
(TUI/web) sessions stay open for further chat turns after each response
(`goal_requires_tddy_tools_submit` returns `false` for `prompting`, since the agent's plain
response, not a `tddy-tools submit` payload, is the result).

`BackendInvokeTask::run` (`packages/tddy-core/src/workflow/backend_invoke_task.rs:333-352`)
handles a goal that doesn't require submit by returning `NextAction::Continue` with the backend's
`response.output`, as long as the recipe's `host_clarification_gate_after_no_submit_turn` doesn't
inject a follow-up question (it doesn't, for `free-prompting`).

`tddy-graph`'s `FlowRunner` (`packages/tddy-graph/src/runner.rs:107-123`) then looks up the next
task for `Continue`. Because `prompting` has no successor, the runner synthesizes
`ExecutionStatus::WaitingForInput` — **structurally identical to an explicit clarification
request**, but *without* `context["pending_questions"]` being set. This is intentional: the TUI
and web presenters have their own loop that feeds the next chat message back into the session
without expecting `pending_questions`.

The plain-mode CLI handlers, however, assume every `WaitingForInput` carries clarification
questions:

- `run_full_workflow_plain` (`packages/tddy-coder/src/run.rs`, `ExecutionStatus::WaitingForInput`
  arm)
- `run_goal_plain` (`packages/tddy-coder/src/run.rs`, `ExecutionStatus::WaitingForInput` arm)

Both do `session.context.get_sync("pending_questions").ok_or_else(|| anyhow!("no pending
questions"))?` unconditionally, so any single-shot `free-prompting` run through the plain CLI path
crashes on the very first turn.

**Confirmed reproduction** (deterministic, no network):

```bash
echo "hello" | tddy-coder --agent stub --recipe free-prompting --prompt "..."
# Error: no pending questions
```

This is not specific to any one backend — `StubBackend::prompting_response()`
(`packages/tddy-core/src/backend/stub.rs:198`) already has a dedicated non-submitting,
no-questions response for the `prompting` goal, and hits the exact same crash as `fastcontext`.
This was discovered while trying to run `tddy-coder --agent fastcontext --recipe free-prompting`
against a real (Ollama-served) FastContext model as a Discovery-style, single-shot query backend.

A third occurrence of the same error string exists in `run_plan_refinement`
(`packages/tddy-coder/src/run.rs`) but is unrelated: plan refinement always sets
`pending_questions` (it's a genuine clarification loop) and is out of scope for this change.

**A second, compounding gap:** even once the crash above is avoided, there is nothing to print.
`BackendInvokeTask::run`'s no-submit/no-question `Continue` branch
(`backend_invoke_task.rs:346-351`) returns `response.output` inside `TaskResult.response`, but
**never calls `context.set_sync("output", ...)`** — unlike the submit-success branch just above it
(`backend_invoke_task.rs:270`), which does. `tddy-graph`'s `FlowRunner` never copies
`TaskResult.response` into the session context on its own (`packages/tddy-graph/src/runner.rs`).
Since the plain CLI only has access to `session.context` after the engine call returns (not the
`TaskResult` itself), `context["output"]` must be populated for the plain CLI to have anything to
print. This is why `packages/tddy-workflow-recipes/tests/backend_invoke_no_tddy_tools_submit.rs`'s
existing `free_prompting_backend_invoke_completes_without_tddy_tools_submit` only ever asserted on
`result.response`, never on the context.

**Also note:** `--goal prompting` is not a reachable CLI input — `CoderArgs::goal`'s clap
`value_parser` only allows a fixed list (`plan`, `reproduce`, `acceptance-tests`, `red`, `green`,
`post-green-review`, `demo`, `evaluate`, `validate`, `refactor`, `update-docs`), which does not
include `prompting`. In practice, `free-prompting`'s single `prompting` goal is only ever reached
through the full-workflow path (`run_full_workflow_plain` / `run_full_workflow_tui`), never through
`run_goal_plain`. `run_goal_plain` shares the identical buggy pattern and is fixed for consistency,
but no CLI-reachable scenario exists today that exercises its "Continue with no successor, no
questions" branch specifically — the plain CLI acceptance coverage for that arm's *unaffected*
behavior (a genuine clarification pause) is what matters, and is exercised via the `plan` goal.

## Requirements

### Functional Requirements

- [x] `BackendInvokeTask::run`'s no-submit, no-clarification `Continue` branch
      (`packages/tddy-core/src/workflow/backend_invoke_task.rs:346-351`) persists the backend's
      response into `context["output"]`, matching the existing submit-success branch's behavior.
- [x] When plain-mode workflow execution reaches `ExecutionStatus::WaitingForInput` and the
      session context has **no** (or an empty) `pending_questions` value, the plain CLI treats it
      as workflow completion: print `context["output"]` and return `Ok(())`, instead of erroring.
- [x] This applies to both `run_full_workflow_plain` and `run_goal_plain`
      (`packages/tddy-coder/src/run.rs`) — the two plain-mode entry points that currently share the
      identical `WaitingForInput` → `pending_questions` bug.
- [x] `run_plan_refinement`'s `WaitingForInput` handling is unchanged — plan refinement always
      carries `pending_questions`, so it is not part of this fix's scope.
- [x] Existing `WaitingForInput` behavior for goals/recipes that genuinely set `pending_questions`
      (TDD's interview/plan clarification, acceptance-tests permission questions, etc.) is
      unaffected — the plain CLI still reads and answers those questions exactly as before.

### Non-Functional Requirements

- [x] No change to TUI mode (`run_full_workflow_tui`) or daemon/web session flows — those already
      have presenter-driven loops for continuing chat and render output as it streams; they don't
      rely on `context["output"]` for `free-prompting` and are unaffected.
- [x] No change to `FreePromptingRecipe`'s graph/task design or `tddy-graph`'s `FlowRunner` — the
      fix is limited to `BackendInvokeTask`'s existing branch (populate a context key it already
      has direct access to) and the plain CLI's existing `pending_questions` signal.

## Acceptance Criteria

- [x] Running `tddy-coder --agent stub --recipe free-prompting --prompt "..."` in plain
      (non-interactive stdin/stderr) mode exits `0` and the combined stdout/stderr contains the
      backend's response text, instead of `Error: no pending questions`.
- [x] A recipe/goal that legitimately sets `pending_questions` on `WaitingForInput` (e.g. the
      `stub` backend's `plan` clarification flow under the `tdd` recipe) is unaffected — the plain
      CLI still prompts for and reads answers on stdin, and the run still completes successfully.
- [x] `free_prompting_backend_invoke_completes_without_tddy_tools_submit`'s sibling test in
      `backend_invoke_no_tddy_tools_submit.rs` gains a new case asserting `context["output"]` is
      populated after the no-submit `Continue` path.

## Out of Scope

- Building general multi-turn interactive chat for `free-prompting` in the plain (piped stdin)
  CLI mode. This PRD only makes a **single** free-prompting turn complete instead of crash;
  continuing the conversation from the plain CLI is not addressed here.
- Wiring `FastContextBackend`'s citation output into any other workflow goal.
- Changes to `FastContextBackend`, `tddy-discovery`, or Ollama/model configuration.
