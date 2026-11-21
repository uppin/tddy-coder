# 2026-11-21 — A claude-cli transcript carries the prompt and the agent's reply

**Type:** Feature · **Stack:** `#target-parity` 3/7 (PR #971) · **Packages:** `tddy-session-activity` ·
Feature: [claude-cli-session.md § The transcript holds what was said](../../ft/daemon/claude-cli-session.md#the-transcript-holds-what-was-said)

## Problem

A claude-cli session writes no `acp-transcript.jsonl`, so `ReadSessionTranscript` and the Activities
view held its tool calls and nothing it said. An orchestrator following a child could not read the
child's prompts or replies.

## What changed

- **`tddy-session-activity`**: `service/transcript_messages.rs` — `record_spoken_message`, called by
  `report_session_status_impl` after the conversation rotation is followed. `UserPromptSubmit`'s
  `prompt` is appended as a user-message frame, `Stop`'s `last_assistant_message` as an agent-message
  frame, timestamped, through `append_acp_frame`. No reader changed.

## Decisions

- **Hook payload, not Claude's transcript file**: available on a target too, with no file read over
  the shell.
- **Own conversation only**: the last recorded `agent_conversations` id, else the session id; an agent
  tab's hooks are ignored.
- **Live tail deferred**: `AgentActivityHub` carries activity records only — see
  `docs/dev/todo/2026-11-21-transcript-text-has-no-live-tail.md`.

## Wrap-time measurements

- `service/transcript_messages.rs` (new): 54 production lines. `service/hook_reports.rs`: 386 → 387
  (stack-shared, not split).
