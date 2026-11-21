# Changeset: A target agent's question is answered by its orchestrator

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — three crates (agent-backend, agent-launch, session-activity), the channel's second decision kind
**Stack**: `#target-parity` 6/7 — branch `feature/target-parity/question-answer`, base `feature/target-parity/startup-liveness`

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

- [ ] Waiting question entry on a target
- [ ] Question decisions on the channel
- [ ] Feature doc updated through the wrap

## Technical Changes

### State A (Current)

`claude_hooks.rs:181` skips the `AskUserQuestion` entry on a target; after n1 only permission decisions travel the channel.

### State B (Target)

A remote question is the pending question, and its answer reaches the agent as Claude's `updatedInput` decision.

## Acceptance Tests

`packages/tddy-agent-launch/tests/remote_hooks_acceptance.rs`:
- [ ] `a_question_of_an_agent_on_a_target_becomes_the_sessions_pending_question`
- [ ] `an_orchestrators_answer_to_a_target_agents_question_is_printed_by_its_hook`
- [ ] `a_question_nobody_answers_leaves_the_target_agent_to_ask_in_its_dialog`

## TODO

- [x] Record initial discovery (`2026-11-21-target-question-answer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
