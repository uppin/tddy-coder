# Changeset: A target agent's question is answered by its orchestrator

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — three crates (agent-backend, agent-launch, session-activity), the channel's second decision kind
**Stack**: `#target-parity` 6/7 — branch `feature/target-parity/question-answer`, base `feature/target-parity/startup-liveness` — PR [#974](https://github.com/uppin/tddy-coder/pull/974)

## Initial Discovery

[2026-11-21-target-question-answer-initial-discovery.md](./2026-11-21-target-question-answer-initial-discovery.md)

## Affected Packages

- **tddy-agent-backend**: the waiting `PreToolUse` `AskUserQuestion` entry installed on a target
- **tddy-agent-launch**: the follower's hold produces a question decision for a question line
- **tddy-session-activity**: `question_decision_json` moves here from tddy-tools

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-question-answer.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-question-answer.md)

## Summary

The answer channel built by n1 carries a second decision kind: the chosen answers to an `AskUserQuestion`.

## Responsibility

- The waiting question entry on a target.
- Question decisions in the follower's hold; `question_decision_json` at its new home.

## Boundaries

- Does not change the channel, the nonce or the permission decisions (n1).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` permission-answer | the waiting remote hook command, `HookLine::answer_nonce`, the follower's hold and answer-file write, `HookReportDelivery`'s hold | installs the same command for the question matcher and adds the question decision | change any of them |
| `n4` running-tool-calls | the catch-all reporting `PreToolUse` | its report of a question tool stays activity-only | — |

## Draft PR contract

The first push publishes `question_decision_json` at its new home as `todo!()`-bodied surface, plus the failing tests below.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** no — its tests answer through n1's channel
**Concurrent with:** n7
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Scope

- [x] Waiting question entry on a target
- [x] Question decisions on the channel
- [ ] Feature doc updated through the wrap

## Technical Changes

### State A (Current)

`claude_hooks.rs:181` skips the `AskUserQuestion` entry on a target; after n1 only permission decisions travel the channel.

### State B (Target)

A remote question is the pending question, and its answer reaches the agent as Claude's `updatedInput` decision.

## Acceptance Tests

`packages/tddy-agent-launch/tests/remote_hooks_acceptance.rs` (the `AnOrchestrator` harness of n1; `installed_hook_for_tool` now prefers a group naming the tool over a catch-all):
- [ ] `a_question_of_an_agent_on_a_target_is_held_for_its_orchestrators_answer`
- [ ] `an_orchestrators_answer_to_a_target_agents_question_is_printed_by_its_hook`
- [ ] `a_question_nobody_answers_leaves_the_target_agent_to_ask_in_its_dialog`

`packages/tddy-coding-agent-specs/tests/remote_hooks_render_acceptance.rs`: n1's `claude_code_on_a_target_waits_for_a_permission_answer_in_its_worktree` now expects `PreToolUse` among the waiting events.

`packages/tddy-agent-backend/src/claude_hooks.rs` (unit):
- [ ] `a_target_waits_for_a_question_answer_beside_its_hook_log`

`packages/tddy-session-activity/src/hook_decision.rs` (unit):
- [ ] `an_answered_question_is_handed_to_claude_in_its_own_input`

## TODO

- [x] Record initial discovery (`2026-11-21-target-question-answer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (delegated: the user said "finish the rest without approval")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
