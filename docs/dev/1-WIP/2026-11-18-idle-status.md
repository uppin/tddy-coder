# Changeset: An idle session is reported as waiting for input, and a message to it is accepted

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Fix
**Complexity**: 4 — two crates (`tddy-session-activity`, `tddy-daemon-rpc`), hook → status → tool-refusal path, integration tests
**PR**: #946 · **Stack**: `#coder-start` 6/10 — branch `feature/coder-start/idle-status`, base `feature/coder-start/coder-transport`

## Initial Discovery

[`2026-11-18-idle-status-initial-discovery.md`](2026-11-18-idle-status-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE — [`2026-11-18-idle-session-reported-as-waiting-on-a-permission.md`](../todo/2026-11-18-idle-session-reported-as-waiting-on-a-permission.md) (the permission-wait part; the terminal/activity oddities it mentions are out of scope and noted in the wrap)

## Affected Packages

- `tddy-session-activity`: `src/service/hook_reports.rs` (`record_pending_question`, `question_asked_by`), `tests/pending_question_capture_acceptance.rs`.
- `tddy-daemon-rpc`: `src/exec_tool/project_home/turn.rs` (`refuse_while_waiting_on_an_answer`), `src/exec_tool/watch_wake.rs` (idle test), `tests/project_home_turn_control_acceptance.rs`, `tests/project_home_watch_wake_acceptance.rs`.
- `tddy-changeset`: `PendingQuestion` doc (`request_id` no longer empty by design) — doc only.

## Scope

- [ ] No pending-question record without a request id
- [ ] `SendSessionMessage` refuses only on an answerable question
- [ ] Idle wakes see a waiting-for-input session with nothing to answer as idle
- [x] Reproduced first by a failing test that feeds an `idle_prompt` Notification
- [ ] Scoped gates

## Responsibility

What a hook-reported wait means: a question with an id to answer, or a session waiting for input.

## Boundaries

- No change to how structured questions are captured or answered.
- No proto change; `activity_status` values are unchanged.
- Not the remote hook follower's transport (`coder-transport`).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/coder-start/coder-transport` | nothing this node uses (line neighbour only) | — | — |

## Draft PR contract

Commit 2: failing tests only — `an_idle_prompt_leaves_the_session_waiting_for_input_with_nothing_to_answer`
(session-activity), `a_message_to_a_session_waiting_for_input_is_delivered` and
`a_session_waiting_for_input_with_nothing_to_answer_wakes_as_idle` (daemon-rpc). No new surface.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — in-process hook reports and exec tools.
**Concurrent with:** `coder-transport`, `target-agent-start`
**Blocks:** `target-test-kit` (only through the line)

## Technical Changes

### State A

Discovery, Exploration 3 § Idle: `hook_reports.rs:140-149` writes the id-less `permission` record
(comment: preserves `#start-parity` 6/9); `turn.rs:133-156` refuses on any record; `watch_wake.rs:290-303`
and `tddy-agent-launch/src/project_home_idle.rs` treat any record as non-idle.

### State B

- `hook_reports.rs`: the no-tool, no-kept-question branch sets `WaitingForInput` (and the plain
  notification) without writing a record.
- `turn.rs`: refuse only when the record has a request id. (A record on disk from before this fix is
  the only id-less one left; it is also cleared by the next PostToolUse/UserPromptSubmit/Stop.)

## Red phase (commit 2)

Failing tests, each failing for the missing fix:
- `tddy-session-activity/tests/pending_question_capture_acceptance.rs:553`, `an_idle_prompt_leaves_the_session_waiting_for_input_with_nothing_to_answer`. It also asserts (565-568) that the plain `ActivityStatus` notification is still published, which already passes. The test fails on the id-less `permission` record.
- `tddy-daemon-rpc/tests/project_home_turn_control_acceptance.rs:402`, `a_message_to_a_session_waiting_for_input_is_delivered`. It fails with `waiting on permission ""`.
- `tddy-daemon-rpc/tests/project_home_watch_wake_acceptance.rs:343`, `a_session_waiting_for_input_with_nothing_to_answer_wakes_as_idle`. It fails because the orchestrator is never woken.

The request in test 1 is what `tddy-tools session-hook` sends for an `idle_prompt`, because the `notification_type` → `WaitingForInput` mapping lives in `tddy-tools`.

**Pre-existing failures (not this node's):** the baseline `./test -p tddy-session-activity -p tddy-daemon-rpc` ran 930 tests: 930 passed, 7 failed, 1 ignored. The 7 failures are all macOS seatbelt jail tests (`sandboxed_codebase_{lifecycle,placement}_exec_tool_acceptance`, `sandboxed_codebase_seatbelt_acceptance`), the known "sandbox RPC bridge not installed" harness gap.

## Decisions & Trade-offs

- **No `#start-parity` 6/9 test pins the id-less record.** `tddy-tools/tests/session_hook_pending_question.rs` checks only the request. The `hook_reports.rs` comment that cites 6/9 is rewritten at green.
- **Removing the record alone fixes the idle wake.** `watch_wake.rs` already treats only `pending.is_some()` as non-idle. The `turn.rs` change (refuse only an answerable record) covers a stale id-less record already on disk.

- ⚠ **`#start-parity` 6/9 behaviour.** The branch being removed was kept deliberately to surface a
  Notification-only wait as a question. If a 6/9 test pins that record, commit 2 changes its
  expectation and 6e asks the developer.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [x] Draft PR contract — failing tests (commit 2)
- [x] USER REVIEW — acceptance tests (6e)
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap
