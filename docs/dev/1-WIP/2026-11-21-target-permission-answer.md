# Changeset: A target agent's permission prompt is answered by its orchestrator

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — four crates (agent-backend, agent-launch, session-activity, tools) plus lifecycle wiring; a daemon-side hold on concurrent follower lines; a delivery-port signature change; the hook's shell protocol is new
**Stack**: `#target-parity` 1/7 — branch `feature/target-parity/permission-answer`, base `master` — PR [#969](https://github.com/uppin/tddy-coder/pull/969)

## Initial Discovery

[2026-11-21-target-permission-answer-initial-discovery.md](./2026-11-21-target-permission-answer-initial-discovery.md)

## Affected Packages

- **tddy-agent-backend**: the waiting remote hook command; `build_claude_hooks_settings` installs `PermissionRequest` on a target (with Claude's `timeout`)
- **tddy-agent-launch**: `HookLine` carries the answer nonce; the follower holds for the answer and writes `.tddy/answers/<nonce>`; the answers directory is excluded from git and removed with the log
- **tddy-session-activity**: `permission_decision_json` moves here from tddy-tools; the hold the follower uses is the one `AwaitQuestionAnswer` uses
- **tddy-tools**: `session-hook` uses the moved decision builder
- **tddy-session-lifecycle**: `ActivityHookDelivery` returns the report's `request_id` and serves the hold (adapter landed with the contract)
- **tddy-session-driver**: its recording `HookReportDelivery` answers "no orchestrator" (adapter landed with the contract)
- **tddy-coding-agent-specs**: the target render acceptance test now pins the waiting permission hook

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-permission-answer.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-permission-answer.md)

## Summary

A permission prompt of an agent on a target is reported, recorded and answered like a local one. The hook on the target appends its line with a nonce and waits for `.tddy/answers/<nonce>`. The daemon reports it, holds for the orchestrator's answer, and writes Claude's decision there through the session's shell.

## Responsibility

- The waiting remote hook command (POSIX only): mint a nonce, append `<event>\t<token>\t<payload>` with the nonce, poll `.tddy/answers/<nonce>` within `answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS`, `cat` it to stdout; give up silently.
- `build_claude_hooks_settings` with `remote_log`: `PermissionRequest` installed as that command, with `timeout = answer_wait_secs + CLAUDE_HOOK_TIMEOUT_MARGIN_SECS`.
- `HookLine.answer_nonce` and its parsing; a line with a nonce is a waiting hook.
- The follower's hold: report → `request_id` → the same wait `AwaitQuestionAnswer` serves (presence, recorded hold) → decision JSON → atomic write of the answer file (temp + `mv`) through the session's shell; spawned so the reader never stalls.
- `HookReportDelivery::report_session_status` returns the `request_id`; a hold method on the port.
- `permission_decision_json` in tddy-session-activity (moved; tddy-tools re-uses it).
- `.tddy/answers/` in the worktree's git exclude, removed by `remove_remote_hook_log`.

## Boundaries

- Does not install the `AskUserQuestion` waiting hook on a target (`question-answer`, n6).
- Does not change local hooks, `AwaitQuestionAnswer`'s RPC or `AnswerSessionPermission`'s shape.
- Does not install a catch-all `PreToolUse` (`running-tool-calls`, n4).
- No tddy software on the target, and no socket forwarded to it.

## Dependencies

None — root node, based on `master`.

## Draft PR contract

The first push publishes `remote_answering_hook_command` (tddy-agent-backend), `HookLine::answer_nonce`, the changed `HookReportDelivery` signatures and `permission_decision_json` at its new home as `todo!()`-bodied surface, plus the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — its tests use a stand-in target and an in-process activity service
**Concurrent with:** n2, n3, n4, n5
**Blocks:** n6 `question-answer` (its question decisions ride this channel)

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Prerequisites

### ⚠ DURING — [2026-11-19-coder-remote-agent-local-paths.md](../todo/2026-11-19-coder-remote-agent-local-paths.md)

The answer channel must name no host path on the target: the nonce, the log and the answers directory are all relative to the worktree's `.tddy/`.

### ℹ ANSWERED (narrow at wrap) — [2026-11-16-coder-hosts-never-run-against-a-real-deployment.md](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)

The 2026-11-21 probe on `mantasi/remote-dev` delivered SessionStart, UserPromptSubmit, PostToolUse, Notification and Stop through the follower: the "hook follower on a Coder workspace" fact now has real-deployment evidence.

## Scope

- [x] Waiting remote hook command + its settings entry
- [x] Nonce on the hook line; follower hold and answer-file write
- [x] Delivery port returns the request id and serves the hold
- [x] Decision JSON moved to tddy-session-activity
- [x] Answers directory excluded and removed
- [ ] Feature doc `claude-cli-session.md` § "On a target session" updated through the wrap

## Technical Changes

### State A (Current)

`claude_hooks.rs:181` skips `PermissionRequest` on a target. The follower (`remote_hook_follower.rs`) delivers report-only lines through `ActivityHookSink`; `ActivityHookDelivery` discards the response, so the `request_id` is lost. `permission_decision_json` is in `tddy-tools/src/session_hook.rs:109`, out of reach of the daemon crates.

### State B (Target)

The target's settings carry a waiting `PermissionRequest`. Its line reaches the daemon with a nonce; the daemon records the question, holds, and writes the decision file; the hook prints it. No answer → no file → Claude's terminal prompt.

## Acceptance Tests

`packages/tddy-agent-launch/tests/remote_hooks_acceptance.rs` (stand-in target, real hook commands under `sh`):
- [x] `a_permission_request_of_an_agent_on_a_target_reaches_the_daemon_with_its_tool_and_input` (reproduction, exists)
- [x] `an_orchestrators_answer_to_a_target_agents_permission_request_is_printed_by_its_hook`
- [x] `an_orchestrators_denial_reaches_the_target_agent_with_its_message`
- [x] `a_permission_request_nobody_answers_leaves_the_target_agent_to_ask_in_its_terminal`
- [x] `ending_a_target_session_removes_its_answers_on_the_target`

`packages/tddy-coding-agent-specs/tests/remote_hooks_render_acceptance.rs`:
- [x] `claude_code_on_a_target_waits_for_a_permission_answer_in_its_worktree` (replaces `no_hook_that_waits_for_an_answer_is_installed_on_a_target`)

`packages/tddy-agent-backend/src/remote_hook_command.rs` (unit, real `sh`):
- [x] `the_waiting_command_prints_the_answer_written_for_its_nonce`
- [x] `an_empty_answer_ends_the_waiting_command_at_once_with_no_output`
- [x] `the_waiting_command_gives_up_silently_when_no_answer_comes`
- [x] `the_waiting_command_names_no_tddy_binary`

`packages/tddy-session-activity/src/hook_decision.rs` (unit): allow, deny with message, no orchestrator → no decision.

`packages/tddy-agent-launch/src/remote_hook_follower.rs` (unit): `a_waiting_hooks_line_carries_its_nonce`.

**Line format of a waiting hook** (approved at review): `<event>@<nonce>\t<token>\t<payload>`; report-only lines are unchanged.

## Decisions & Trade-offs

- **The hold is daemon-side**, because the `request_id` is minted there and `AwaitQuestionAnswer` requires it.
- **One node, not "visible" then "answerable".** A visible-only first step would ship an `AnswerSessionPermission` that succeeds while delivering nothing. Complexity 5 is recorded rather than split.
- **Atomic answer write** (temp + `mv`), because `RemoteShell::write_file` is `mkdir -p && cat >` and a poller could read half a file.

## TODO

- [x] Record initial discovery (`2026-11-21-target-permission-answer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
