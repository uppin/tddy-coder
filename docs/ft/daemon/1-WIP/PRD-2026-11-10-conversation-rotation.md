# A session follows its agent through `/clear` — PRD

**Date**: 2026-11-10
**PRD Type**: Bug Fix
**Stack**: `#orchestrators` 6/11 (`feature/orchestrators/conversation-rotation`)

## Affected Features

- **Primary**: [Claude CLI session](../claude-cli-session.md) — resume continues the newest conversation
- [Coding agent specs](../../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md) — a declared conversation-rotation slot
- [Agent session status](../agent-session-status.md) — the session-start hook reports the conversation

## Summary

When an agent starts a fresh conversation inside a running session — `/clear` in Claude Code — the
daemon records the new conversation id, and every later resume continues **that** conversation, not the
one the session started with. Every conversation id the session has had is kept, in order, in
`.session.yaml` (`agent_conversations: [{ id, started_at_unix_ms, reason: start | rotate }]`); the last
is current. The mechanism is generic: an agent's YAML spec declares how a rotation is reported
as two hook reports, `conversation.rotation: { ends: <report>, begins: <report> }`, each
`{ hook: <event>, when: { <field>: <value> }, id_field: <field> }`; claude's declares its session-end
hook with `reason: clear` and its session-start hook with `source: clear`, both with `session_id`.
A rotation happens only when the `ends` names the session's current conversation and the `begins`
follows it, so a clear in a claude-cli agent tab — which shares the session's hooks — rotates
nothing. No agent-specific code.

## Background

Claude's conversation id is the tddy session id (`--session-id` / `--resume <tddy id>`). After `/clear`
the agent runs a new conversation, but a resume — including main's resume by `EnsureProjectHomeSession`
after auto-stop — brings back the conversation from before the clear.

## Proposed Changes

### What's Changing
- `tddy-tools session-hook` forwards the hook's conversation id and the fields the spec names;
  `ReportSessionStatusRequest` carries them.
- The daemon appends a rotation to `agent_conversations` (deduplicated: the same id twice is one entry).
- Resume binds the newest id where the spec's conversation kind uses one; transcript and usage readers
  read the current conversation.
- A session with no list resumes as today.

### What's Staying the Same
- Session id, record, worktree. Agents whose spec declares no rotation.

## Impact Analysis
- `SessionMetadata` refuses unknown fields: an older daemon cannot read a record carrying the list.

## Acceptance Criteria
- [ ] After `/clear` in a claude-cli session, the record lists both conversations, the new one last
- [ ] Resuming that session continues the post-clear conversation
- [ ] Main auto-stopped after a `/clear` resumes the post-clear conversation
- [ ] A `/clear` in an agent tab does not rotate the session's conversation
- [ ] An agent spec without a rotation slot is unaffected; a spec naming an unknown hook is refused at load

## References
- [Claude CLI session](../claude-cli-session.md), [Project-home auto-stop](../project-home-auto-stop.md)
