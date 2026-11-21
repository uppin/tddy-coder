# Changeset: A claude-cli tool call is reported running before it runs

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — three crates (agent-backend, tools, session-activity / agent-launch follower), the hook-matcher contract changes for local and target sessions
**Stack**: `#target-parity` 4/7 — branch `feature/target-parity/running-tool-calls`, base `feature/target-parity/transcript-text`

## Initial Discovery

[2026-11-21-claude-cli-running-tool-calls-initial-discovery.md](./2026-11-21-claude-cli-running-tool-calls-initial-discovery.md)

## Affected Packages

- **tddy-agent-backend**: `build_claude_hooks_settings` installs a reporting `PreToolUse` entry for every tool (matcher `""`), beside the waiting `AskUserQuestion` entry where that one is installed
- **tddy-tools**: `session-hook` reports no status for a `PreToolUse` that does not wait for an answer
- **tddy-agent-launch**: the follower likewise reports no status for a `PreToolUse` line without an answer nonce

## Related Feature Documentation

- PRD: [PRD-2026-11-21-claude-cli-running-tool-calls.md](../../ft/daemon/1-WIP/PRD-2026-11-21-claude-cli-running-tool-calls.md)

## Summary

The catch-all `PreToolUse` report writes the `running` row (paired by `tool_use_id`) and nothing else, so the question tools stay recorded once, by their waiting hook.

## Responsibility

- The catch-all reporting `PreToolUse` entry, local and target.
- "A `PreToolUse` that does not wait reports activity only" in `session-hook` and in the follower.

## Boundaries

- Does not change how questions or permissions are recorded or answered.
- Does not change the activity row format or pairing (`tool_use_id` already pairs).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` permission-answer | `HookLine::answer_nonce` | the follower tells a waiting `PreToolUse` from a reporting one by it | change the nonce or the channel |

## Draft PR contract

No new public surface; the first push is the failing tests below.

## Green wave

**Wave:** 1 of 2 (reads n1's published `answer_nonce` field only; no n1 behaviour is exercised)
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n5
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Scope

- [ ] Catch-all entry in the settings builder
- [ ] Activity-only reporting PreToolUse (local hook and follower)
- [ ] Feature doc updated through the wrap

## Technical Changes

### State A (Current)

`CLAUDE_HOOK_EVENTS` holds `("PreToolUse", "AskUserQuestion")` only (`claude_hooks.rs:90`), skipped on a target. No claude-cli row is ever `running`; every `started_unix_ms` is 0.

### State B (Target)

Every tool call gets a `running` row with its start time, completed by its `PostToolUse`; questions unchanged.

## Acceptance Tests

`packages/tddy-agent-launch/tests/remote_hooks_acceptance.rs`:
- [ ] `a_tool_call_of_an_agent_on_a_target_is_reported_before_it_runs` (reproduction, exists)

`packages/tddy-session-activity/tests/` (real handler, real session directory):
- [ ] `a_tool_call_is_running_with_its_start_time_until_its_result_completes_it`
- [ ] `a_question_tools_reporting_pre_tool_use_records_no_question_and_no_call`

`packages/tddy-agent-backend/src/claude_hooks.rs` (unit):
- [ ] `every_session_reports_every_tool_before_it_runs` (local and target settings)

## Decisions & Trade-offs

- **Local too** (user, 2026-11-21): one extra short `tddy-tools` process per tool call locally is accepted for the same behaviour everywhere.
- **Activity-only catch-all** rather than a regex matcher excluding the question tools: Claude's matcher syntax is not something this repo pins, the client-side rule is.

## TODO

- [x] Record initial discovery (`2026-11-21-claude-cli-running-tool-calls-initial-discovery.md`)
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
