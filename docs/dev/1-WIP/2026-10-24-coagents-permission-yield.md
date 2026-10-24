# Changeset: A coding subagent's permission request is the main agent's to answer

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto change (chunk + resume fields, stop-reason tables on both ends), concurrency (a parked ACP permission future resolved from a later call), three crates
**Branch**: `feature/coagents/permission-yield`, base `feature/coagents/acp-subagent` (#721) — `#coagents` 3/8
**PR**: [#722](https://github.com/uppin/tddy-coder/pull/722)

## Initial Discovery

[2026-10-24-coagents-permission-yield-initial-discovery.md](./2026-10-24-coagents-permission-yield-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ⚠ DURING — `subagent_end` can race a prompt — [`2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md`](../todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md)

An end arriving while a permission is parked takes the cancel path, which answers the parked
request as cancelled first. The race itself is not fixed here.

### ⚠ DURING — The resume RPC and its turn budget have no wire-level test — [`2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`](../todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md)

This node adds a wire-level test for the resume's `permission_answer_json` (test 8); the turn-budget
part stays open.

## Responsibility

- `StopReason::PermissionRequested` (serde `permission_requested`) and its spelling in both wire
  tables (`tddy-session-agents/src/service.rs` stop reasons, `tddy-discovery/src/roster/conversation.rs`).
- `PromptOutcome.permission_request: Option<PermissionRequestView>` — `{ request_id, tool_call: { title, kind, raw_input }, options: [{ option_id, name, kind }] }`.
- `TurnRequest.permission_answer: Option<PermissionAnswer>` — `Selected { request_id, option_id }` / `Cancelled { request_id }`.
- `tddy_coding_subagent::ParkedPermissions` — the `PermissionHandler` (n1's port) that parks a request,
  lets the in-flight `AcpDriver::prompt` keep running, and ends the *tddy turn* with the request;
  `CodingAgentSession::take_turn` resolves it from `permission_answer` and continues waiting on the same
  ACP prompt. Replaces n2's `DenyAndRecord` for subagent conversations.
- Refusals: prompt while pending; resume without/with a foreign `request_id`.
- Cancel while pending answers `Cancelled`, then `session/cancel`.
- Proto: `AgentConversationChunk.permission_request_json` (final frame), `ResumeAgentConversationRequest.permission_answer_json`.
- `tddy-tools`: `subagent_resume` schema/description gain `permission`; `prompt_outcome_json` gains `permissionRequest`.

## Boundaries

- No web UI for answering — n6 `web-approvals`.
- No `-p` permission routing — n4 `headless-driver` maps claude's `--permission-prompt-tool` onto this.
- No auto-allow policies per roster entry.
- `model` agents unchanged.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `PermissionHandler`, `PermissionRequest`, `PermissionDecision`, `AcpDriver::{prompt, cancel}` | implements the handler; awaits the same prompt across tddy turns | change the driver's API |
| n2 `acp-subagent` (#721) | `CodingAgentSession`, `DenyAndRecord`, `SubagentError::UnsupportedByCodingAgent`, the daemon open path | swaps the installed handler; extends `take_turn` with the answer | change the kind wire fields, the catalog or the launcher |

## Draft PR contract

`StopReason::PermissionRequested`, `PermissionRequestView`, `PermissionAnswer`, `ParkedPermissions`,
the proto fields with regenerated code, the `subagent_resume` schema field — bodies
`// TODO(permission-yield): implement` — and the failing tests below.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs n2's `CodingAgentSession` behaviour
**Concurrent with:** n5 `conversation-bubble`, n7 `clone-placement`, n8 `remote-codebase`
**Blocks:** n4 `headless-driver` (claude `-p` permission routing yields through this), n6 `web-approvals`

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-discovery**: `StopReason`, `PromptOutcome`, `TurnRequest`, wire stop-reason table
- **tddy-coding-subagent**: `ParkedPermissions`, `take_turn` continuation
- **tddy-session-agents**: stop-reason table, resume answer forwarding
- **tddy-service**: `session_agents.proto` fields (regenerated TS)
- **tddy-tools**: `subagent_resume` schema + outcome JSON

## Related Feature Documentation

- [PRD-2026-10-24-coagents-permission-yield.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-permission-yield.md)

## Summary

A coding subagent's permission request yields the turn to the main agent; `subagent_resume` answers it.

## Scope

- [ ] **Implementation**
- [ ] **Testing**
- [ ] **Package Documentation**: managed-codebase-subagents § permission requests
- [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `tddy-discovery/src/subagent.rs:80-107` `StopReason { EndTurn, MaxTurnRequests, Cancelled, ContextExhausted, MaxTokens, YieldedToCaller }`; `:111-141` `PromptOutcome`.
- `subagent/turn_request.rs` `TurnRequest` (prompt / resuming, replacement, correction).
- Wire stop reasons spelled in `tddy-session-agents/src/service.rs:424-433` and `roster/conversation.rs:508-526` — unknown spellings refused.
- `session_agents.proto:90-119` `AgentConversationChunk`; `:282-307` `ResumeAgentConversationRequest` (fields 1-9).
- After n2: `CodingAgentSession` installs `DenyAndRecord`.

### State B (Target)

```
prompt ─► AcpDriver::prompt (in flight) ──request_permission──► ParkedPermissions
                                                                    │ park (oneshot)
take_turn returns PromptOutcome{stopReason: permission_requested, permissionRequest}
subagent_resume{permission} ─► take_turn(answer) ─► oneshot.send(option) ─► keep awaiting the same prompt
```

## Testing Plan

**Level:** integration — the parked future and its resolution across two calls only show up against a
real ACP child (`tddy-acp-stub` with a `permission_requests` scenario whose outcome the stub records);
wire tests in `tddy-daemon-rpc` for the remote shape; `tddy-tools-cli` MCP test for the JSON.

## Acceptance Tests

Build the stub first (`cargo build -p tddy-acp-stub`).

`packages/tddy-coding-subagent/tests/permission_yield_acceptance.rs` (`#[serial]`, stub agent asking through `session/request_permission`, outcomes read from its record) — fail at `CodingAgentSession::start` until #720/#721 are green, then on this node's missing `ParkedPermissions`:

1. `a_permission_request_ends_the_call_with_the_request_and_its_options` — `PermissionRequested`, title, `[(allow-once, allow_once), (reject-once, reject_once)]`, a non-empty `requestId`
2. `resuming_with_an_option_continues_the_same_turn_and_the_agent_receives_it` — agent records `selected:allow-once`; the continuation ends `EndTurn` with the turn's text; one `new_session`
3. `a_cancelled_answer_reaches_the_agent_as_cancelled`
4. `a_second_request_in_the_same_turn_yields_again` — a fresh `requestId`; both answers reach the agent in order
5. `a_prompt_while_a_request_is_pending_is_refused_naming_it`
6. `a_resume_naming_another_request_is_refused_naming_the_pending_one` — and a resume answering nothing, likewise; the agent got no answer
7. `cancelling_while_a_request_is_pending_returns_cancelled` — the agent records `cancelled` and one `cancel`

`packages/tddy-tools/tests/session_agent_conversation_client_acceptance.rs` (extended; its fake daemon learns `ResumeAgentConversation`) — the in-jail client half of the wire:

8. `a_final_frame_naming_a_permission_request_yields_it_to_the_caller` — fails: `permission_request` not parsed
9. `a_resume_carries_the_callers_permission_answer` — fails: `permission_answer_json` empty; expected `{"answer":"selected","requestId":"perm-1","optionId":"allow-once"}`

`packages/tddy-session-agents/tests/permission_request_frames.rs` — the daemon half:

10. `a_permission_request_rides_the_final_frame` — fails: `permission_request_json` empty

`packages/tddy-discovery/tests/permission_outcome_json_acceptance.rs` — the MCP outcome:

11. `a_paused_turn_names_its_stop_reason_and_its_permission_request` — fails: no `permissionRequest`
12. `an_ordinary_turn_carries_no_permission_request` — **passes at contract** (negative)

`packages/tddy-tools/tests/permission_answer_arg_acceptance.rs` — `subagent_resume`'s `permission` argument:

13. `an_option_is_a_selected_answer` · 14. `cancelled_is_a_cancelled_answer` · 15. `no_permission_is_no_answer` · 16. `both_an_option_and_cancelled_is_refused_naming_both` · 17. `an_answer_without_a_request_id_is_refused_naming_it` — all fail at `parse_permission_answer`'s TODO

**Changed from the plan:** the daemon-RPC round trip (old test 8) is split into its two halves — the client (8, 9) and the daemon frames (10) — because the whole round trip needs #721's recording-launcher harness, a file that node owns; the full path is exercised end to end once both are green. The MCP-level tests (old 9, 10) are 11 and 13–17: the outcome JSON and the argument parse are the MCP surface, and the grace / `responseId` path is generic and already covered. Wire spellings published (implemented, not TODO): `PermissionRequested` in both stop-reason tables.

**Changed in green (2026-10-24, developer's decision):** #721's `packages/tddy-coding-subagent/tests/coding_agent_session_acceptance.rs`
`a_permission_request_is_denied_and_recorded_in_the_turns_messages` pinned #721's planned stopgap (deny until this node
hands the request to the main agent). It is **rewritten**, not deleted, as `a_permission_request_yields_instead_of_being_auto_answered`:
the turn ends `PermissionRequested` with the request's title and options, and the stub records no permission outcome yet.

**Baseline (2026-10-24):** `cargo check --all-targets` clean for `tddy-discovery tddy-coding-subagent tddy-session-agents tddy-tools tddy-daemon-rpc tddy-session-lifecycle tddy-tools-cli tddy-daemon tddy-session-split tddy-acp`; scoped clippy `-D warnings` clean; the 16 existing tests in the extended client suite pass.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **The ACP prompt stays in flight across tddy turns.** The alternative — cancel and re-prompt with the
  decision as text — loses the agent's state and is not what the agent asked for.
- **Always yield.** No per-entry auto-allow list: the user chose that the invoking agent decides.
- **Cancel waits for the cancelled turn to end, bounded.** `CancelHandle::cancel` answers the parked requests as
  cancelled, sends `session/cancel`, then waits for the turn to end. Test 7 reads the stub's record as soon as
  `cancel()` returns, and the stub records asynchronously, so an unbounded return fails it. The wait is capped by
  `CANCEL_SETTLE_DEADLINE` (10 s, `packages/tddy-coding-subagent/src/session.rs`): past it `cancel` returns `Ok`, the
  turn is left unsettled, and the session's drop stops the agent process. Residual risk: a silent agent costs at most
  that deadline, not an unbounded hang of `subagent_cancel`.
- **`encode_permission_answer` returns `Result<String, String>`**, not the stubbed `String`. An encode failure
  propagates through `?`; it does not turn into an empty "answers nothing" value.
- **Judgment calls in the green implementation:** parked requests queue FIFO and the caller sees one at a time; an
  answer naming an option the request did not offer is refused and the request stays pending; a resume carrying an
  answer with nothing pending is refused; a new prompt is refused while a cancelled turn's result is uncollected.
- **Tool kind is always `other` for now.** n1's `PermissionRequest` drops the ACP tool kind. The fix belongs to #720's
  driver and is recorded in
  [`docs/dev/todo/2026-10-24-acp-permission-request-drops-tool-kind.md`](../todo/2026-10-24-acp-permission-request-drops-tool-kind.md);
  the FIXME at `packages/tddy-coding-subagent/src/parked_permissions.rs` points there.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-permission-yield-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (16 of 17 contract tests green; the n2 test above is red pending the developer's decision — not ticked)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-coding-subagent -p tddy-discovery -p tddy-session-agents -p tddy-daemon-rpc -p tddy-tools-cli`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
