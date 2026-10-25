# PRD: read a session's activity, transcript and pending question, paged

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 6/9 — branch `feature/start-parity/session-follow`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

An orchestrator can see a session's status but not what it is doing or asking. A child stuck in
WaitingForInput is a dead end. This node adds **`ReadSessionActivity`**, with the open pending
question, and **`ReadSessionTranscript`**. Both are paged under one explicit contract and work on
any host.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- **Paging contract** (developer requirement), the same for both tools:
  - in: `cursor` (empty = the start) and `limit` (default 50, hard maximum 200; a larger value is
    clamped, and the response says so);
  - out: `items`, `next_cursor` and `has_more`;
  - there is no unpaged call.
- `ReadSessionActivity {session_id, daemon_instance_id?, cursor?, limit?, kinds?}` pages forward by
  `activity_seq` ("everything since X"), so a poll resumes where the last one stopped. The response
  carries `pending_question: {kind, header, question, options, allow_other} | null`.
- `ReadSessionTranscript {session_id, daemon_instance_id?, cursor?, limit?, direction:
  newest_first|from_cursor}` returns the frames `GetAcpReplayPage` returns, each item cut at 4 KiB
  with `truncated: true`. `ReadSessionTranscript {…, tool_call_id}` returns one whole item through
  `GetAcpToolCallDetail`.
- **The pending question is persisted** where the flag is set. Tool sessions store the
  elicitation's text next to `pending_elicitation`. claude-cli sessions store it from the
  permission or notification hook that already reports WaitingForInput. It is cleared when
  answered.
- Both tools are limited to the caller's project, and reach another host through `remote-targets`.

### What stays the same

- Does not add user and assistant text to claude-cli transcripts. That history does not exist
  server-side; recorded as a TODO.
- Does not change `StreamAcpReplay` or the UI.

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc, tddy-changeset, tddy-session-activity, tddy-agent-launch (claude-cli hook path), tddy-presenter (tool sessions).
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can read a session's activity page by page and poll for what is new.
- [ ] It can read the session's transcript page by page, and fetch any truncated item whole.
- [ ] It can see the question a session is waiting on.
- [ ] No call returns an unbounded amount of data.
