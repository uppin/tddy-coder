# Changeset: A claude-cli transcript carries the prompt and the agent's reply

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one crate (session-activity), integration tests over a real session directory, the persisted transcript seam
**Stack**: `#target-parity` 3/7 — branch `feature/target-parity/transcript-text`, base `feature/target-parity/target-worktree-actions` — PR [#971](https://github.com/uppin/tddy-coder/pull/971)

## Initial Discovery

[2026-11-21-claude-cli-transcript-text-initial-discovery.md](./2026-11-21-claude-cli-transcript-text-initial-discovery.md)

## Affected Packages

- **tddy-session-activity**: `report_session_status_impl` appends a user-message frame for `UserPromptSubmit`'s `prompt` and an agent-message frame for `Stop`'s `last_assistant_message` to `acp-transcript.jsonl`

## Related Feature Documentation

- PRD: [PRD-2026-11-21-claude-cli-transcript-text.md](../../ft/daemon/1-WIP/PRD-2026-11-21-claude-cli-transcript-text.md)

## Summary

The two texts the hooks already carry are persisted as the ACP frames a tool session writes, so `ReadSessionTranscript` and the Activities view show them without any reader change.

## Responsibility

- Writing the two frames (timestamped) from a status report, for the session's own conversation only.
- Skipping an empty or absent text.

## Boundaries

- Does not read Claude's own transcript file (`transcript_path`); thoughts and intermediate text stay out.
- Does not change the transcript readers, paging or tool-call rows.
- Does not change hooks (both events are already installed, locally and on targets).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1`, `n2` | nothing consumed — the line puts them below | — | — |

## Draft PR contract

No new public surface; the first push is the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes
**Concurrent with:** n1, n2, n4, n5
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Prerequisites

### ✅ RESOLVED HERE — [2026-10-25-claude-cli-transcript-has-no-conversation-text.md](../todo/2026-10-25-claude-cli-transcript-has-no-conversation-text.md)

The `UserPromptSubmit` / `Stop` route the entry names, into `acp-transcript.jsonl`.

### ⚠ DURING — [2026-10-24-claude-agent-tab-shares-session-hooks.md](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)

A Claude started in an agent tab fires the session's hooks. Its prompts and replies must not enter the session's transcript: a text is recorded only when the report's `agent_conversation_id` is the session's own conversation.

## Scope

- [x] Prompt and reply frames written from the status report
- [x] Only the session's own conversation
- [ ] Feature doc updated through the wrap

## Technical Changes

### State A (Current)

`read_session_transcript` (`tddy-service/src/acp_replay.rs:102`) merges `acp-transcript.jsonl` (absent for claude-cli) and `agent-activity.jsonl`. `report_session_status_impl` receives `prompt` / `last_assistant_message` in `hook_fields` and drops them.

### State B (Target)

Both are appended as `user_message_chunk` / `agent_message_chunk` frames and replayed in order among the tool calls.

## Acceptance Tests

`packages/tddy-session-activity/tests/claude_cli_transcript_messages.rs`:
- [x] `the_prompt_and_the_agents_reply_reach_the_transcript` (the reproduction)
- [x] `a_tool_call_keeps_its_place_between_the_prompt_and_the_reply`
- [x] `a_stop_with_no_reply_text_adds_no_agent_message`
- [x] `a_prompt_from_another_conversation_is_not_added` — a **guard**: green today, must stay green. "The session's own conversation" is the last `agent_conversations` entry's id, or the session id when none is recorded (Claude runs with `--session-id` = the session id)

## Decisions & Trade-offs

- **Hook payload, not Claude's transcript file**: available on a target too (no file read over the shell), and enough for the orchestrator's need.

## TODO

- [x] Record initial discovery (`2026-11-21-claude-cli-transcript-text-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
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
