# Changeset: A subagent conversation is one activity bubble that opens the conversation

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto change (three replay requests), three Rust crates plus the web, a persisted transcript store, live-stream ordering
**Branch**: `feature/coagents/conversation-bubble`, base `feature/coagents/permission-yield` (#722) — `#coagents` 4/8
**PR**: [#723](https://github.com/uppin/tddy-coder/pull/723)

## Initial Discovery

[2026-10-24-coagents-conversation-bubble-initial-discovery.md](./2026-10-24-coagents-conversation-bubble-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ✅ RESOLVED HERE — A roster agent's own turns are unobservable from the web — [`2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md`](../todo/2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md)

Resolved for **daemon-run** conversations: frames appended per turn, a conversation axis on the replay
requests. The in-jail loop stays unobservable; wrap narrows the entry to that remainder rather than
deleting it (only the in-jail half is left).

### ⚠ DURING — The agent activity overlay has no production mount — [`2026-10-23-agent-activity-overlay-has-no-production-mount.md`](../todo/2026-10-23-agent-activity-overlay-has-no-production-mount.md)

New specs mount `SessionWithActivitySidebar`, never `AgentActivityOverlay`.

### ⚠ DURING — Tool-detail dialog follow-ups — [`2026-07-26-tddy-web-agent-activity-tool-detail-dialog.md`](../todo/2026-07-26-tddy-web-agent-activity-tool-detail-dialog.md)

The conversation dialog reuses the detail dialog as is; its cache key grows a conversation axis.

## Responsibility

- **Transcript store.** `tddy_service::acp_replay::conversation_transcript_path(session_dir, conversation_id)` and an `append_conversation_frame`; `read_session_transcript` reused per conversation.
- **Recording.** `tddy-session-agents` appends, for every daemon-run conversation turn: the prompt (`user_message_chunk`), streamed text, tool calls / updates, permission requests (n3) and the stop reason. For a coding agent from its `AcpUpdate`s; for a daemon-run `model` agent from its `MessageDescriptor`s.
- **Replay axis.** `conversation_id` on `StreamAcpReplayRequest` (field 6), `GetAcpReplayPageRequest` (6), `GetAcpToolCallDetailRequest` (5); served by `tddy-session-activity` and its peer forward.
- **Folding.** `frame_for_agent_activity` maps the main agent's `subagent_*` hook records carrying a `sessionId` to one ACP tool-call frame with `tool_call_id = "subagent:<conversation id>"`, kind `subagent_conversation`, `_meta.tddy.{conversationId, agentId, label, recorded}`, status from the call (`subagent_prompt`/`await` running → in progress, `permission_requested` → pending, end/cancel → completed, error → failed).
- **Web.** `ChatMessage.from: "subagent"` (+ `conversationId`, `agentLabel`, `recorded`) projected from those frames; `AgentTranscriptView` makes it clickable; `SessionWithActivitySidebar` / `SessionActivitiesPane` open `SubagentConversationDialog` (hosts `useAcpReplay({ sessionId, conversationId })` → `AgentChatView readOnly` → `AgentActivityDetailDialog`); `useAcpReplay`, `useAcpToolCallDetail` and `agentActivityRegistry` keyed by `(sessionId, conversationId)`.

## Boundaries

- No answering a permission from the web — n6 `web-approvals`.
- No recording of in-jail conversations.
- No change to the operator's Agents-tab conversation tabs.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `AcpUpdate` stream from `AcpDriver::prompt` | records it as frames | change the mapping |
| n2 `acp-subagent` (#721) | `CodingAgentSession`, daemon open path, `AgentKind` | taps the session's updates for recording; label/kind on the bubble | change the session or launcher |
| n3 `permission-yield` (#722) | `PermissionRequestView`, `StopReason::PermissionRequested` | records a request frame; bubble status "waiting for permission" | change the yield or the answer |

## Draft PR contract

Proto fields + regenerated code, `conversation_transcript_path`, `append_conversation_frame`, the folding
function signature, `ChatMessage` `subagent` variant, `SubagentConversationDialog` shell — bodies
`TODO(conversation-bubble)` — and the failing tests below.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs n2's daemon-run coding-agent conversation behaviour to have something to record (n3's permission frame for test 3)
**Concurrent with:** n3, n7, n8
**Blocks:** n6 `web-approvals` (answers the pending entry this node renders)

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-service**: `activity.proto`, `acp_replay.rs` (path, append, fold) — regenerated TS
- **tddy-session-agents**: per-turn recording
- **tddy-session-activity**: conversation-addressed replay
- **tddy-session-lifecycle**: peer-forward of the new field
- **tddy-web**: projection, transcript view, dialog, hooks and registry keys

## Related Feature Documentation

- [PRD-2026-10-24-coagents-conversation-bubble.md](../../ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md)

## Summary

Record daemon-run conversations; fold the main agent's `subagent_*` calls into one bubble per
conversation; open the conversation's transcript from it.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** (agent-activity-pane, acp replay) · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `activity.proto:299-306` `StreamAcpReplayRequest { session_token, session_id, daemon_instance_id, mode, page_size }`; `:190-216` page/detail requests — no conversation axis.
- `tddy-service/src/acp_replay.rs:99-112` `read_session_transcript(session_dir)`; `:357-398` `frame_for_agent_activity` — title is the bare tool name for `subagent_*`.
- `tddy-session-activity/src/service/rpc.rs:186-205` `stream_acp_replay` resolves the session dir; `svc_activity_ports.rs:263-273` refuses peer-addressed streams (page/detail forwarded).
- `tddy-session-agents/src/service.rs:162-285` turn chunks go only to the caller; nothing persisted.
- Web: `useAcpReplay.ts:74-78` keyed by `sessionId`; `acpReplayProjection.ts:59-94` one tool bubble per `tool_call_id`; `AgentTranscriptView.tsx:99-103` only `from === "tool"` clickable; `SessionWithActivitySidebar.tsx:87,145-162` one detail state.

### State B (Target)

```
daemon-run turn ─► append_conversation_frame(<session>/conversations/<conv>/acp-transcript.jsonl)
main agent hook records (subagent_*{sessionId}) ─► fold ─► tool_call "subagent:<conv>" (one bubble)
web: bubble(from:"subagent") ─click─► SubagentConversationDialog ─► useAcpReplay({sessionId, conversationId})
```

## Testing Plan

**Level:** integration for the store, recording and RPC (`tddy-session-activity` / `tddy-daemon-rpc`
harnesses with a stub coding agent); unit for the folding; Cypress component for the bubble and
dialog (`mountWithRpc` + `aReplayBackend` extended with a conversation axis).

## Acceptance Tests

Build the stub first (`cargo build -p tddy-acp-stub`).

`packages/tddy-service/tests/subagent_conversation_replay_acceptance.rs` — the store and the fold (`tddy_service::subagent_conversation_replay`):

1. `subagent_calls_for_one_conversation_fold_into_one_tool_call` — `new_session`, `prompt`, `await` on `conv-1` around an unrelated `Read` → `["subagent:conv-1", "r1"]`
2. `the_folded_entry_status_follows_the_last_call` — a running `prompt` after a completed `new_session` → one entry, in progress
3. `subagent_calls_for_two_conversations_are_two_entries`
4. `a_conversation_transcript_reads_back_what_was_appended_in_order`
5. `a_conversation_that_recorded_nothing_reads_as_not_recorded` — **passes at contract** (the TODO answers `None`)

`packages/tddy-session-agents/tests/conversation_transcript_record.rs` — recording a daemon-run turn (`conversation_transcript::record_turn`):

6. `a_finished_turn_records_its_prompt_and_answer` — `["user:where is main?", "agent:main is in src/main.rs"]`
7. `a_turn_paused_on_a_permission_request_records_the_request` — an entry `Permission: Run cargo test`

`packages/tddy-coding-subagent/tests/conversation_updates_acceptance.rs` — live updates (`CodingAgentSession::record_updates_with`; needs #721 green):

8. `every_update_of_a_turn_reaches_the_recorder_in_order`

`packages/tddy-session-lifecycle/src/connection_service/conversation_replay_unit_tests.rs` — the replay RPCs with `conversation_id`, beside the existing activity unit tests:

9. `stream_acp_replay_with_a_conversation_replays_that_conversations_transcript` — the conversation's text, not the session's
10. `a_conversation_that_recorded_nothing_is_not_found_naming_it`
11. `a_conversation_page_is_served_from_the_conversations_transcript`
12. `a_conversation_tool_call_detail_is_served_by_conversation_id`

`packages/tddy-web/cypress/component/SubagentConversationBubbleAcceptance.cy.tsx` (mounts `SessionWithActivitySidebar` over `aConversationReplayBackend`, a per-conversation fake added to `cypress/support/rpc/acpReplay.ts`) — all fail: no `data-message-kind="subagent"` entry yet:

13. `one subagent conversation renders as one bubble with its agent label and status`
14. `clicking the bubble opens the conversation transcript`
15. `a new frame in the conversation appears in the open dialog`
16. `a tool call in the conversation opens its detail`
17. `an unrecorded conversation's bubble says it was not recorded` — `subagent-conversation-not-recorded`

**Changed from the plan:** the daemon-RPC suite moved from a new `tddy-daemon-rpc` file to `tddy-session-lifecycle`'s unit tests, where `ActivityService` is already exercised directly with seeded transcripts — no launcher or coding agent is needed to prove the replay axis. Recording is proved at its seam (6–8) rather than through a whole daemon turn, which needs #721's harness. The "recorded" flag was dropped from the folded entry: an unrecorded conversation is told apart by the replay's `NOT_FOUND` (10, 17), which needs no field the fold could get wrong. Published surface also includes `SUBAGENT_TOOL_CALL_PREFIX` (Rust and TS), `conversation_transcript_dir`, `subagentConversationIdOf`, `AgentTranscriptView.onSubagentClick`, `useAcpReplay({ conversationId })`, `SubagentConversationDialog`.

**Baseline (2026-10-24):** `cargo check --all-targets` clean for `tddy-service tddy-session-activity tddy-session-lifecycle tddy-coder tddy-daemon-rpc tddy-session-agents tddy-coding-subagent`; scoped clippy `-D warnings` clean; the existing activity unit tests compile with the new field.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Fold server-side.** Stream frames are body-stripped, so the web cannot see `sessionId`; folding in
  `frame_for_agent_activity` keeps the web a pure projection.
- **A dialog, not a tab.** Tabs are the operator's own conversations; a dialog matches the detail
  dialog the pane already uses.

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

- [x] Record initial discovery (`2026-10-24-coagents-conversation-bubble-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-service -p tddy-session-agents -p tddy-session-activity -p tddy-daemon-rpc`; the new Cypress spec) — verify 100% pass
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
