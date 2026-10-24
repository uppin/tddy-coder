# A coding agent as a session subagent, over ACP - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 2/8, branch `feature/coagents/acp-subagent`, base `feature/coagents/acp-driver` (#720)

## Affected Features

- **Primary Feature**: [Session agent roster](../session-agent-roster.md) — a roster entry can be a **coding agent**, not only an OpenAI-loop definition
- **Related Feature**: [Harness profiles](../harness-profiles.md) — ACP harness profiles become attachable subagents
- **Related Feature**: [Managed-codebase subagents](../../coder/managed-codebase-subagents.md) — the `subagent_*` MCP tools reach a coding agent
- **Related Feature**: [Session agent catalog fan-out](../../web/session-agent-catalog-fan-out.md) — the create-session picker and Add-agent picker list coding agents

## Summary

A session's main agent can already open conversations with **specialized agents** — OpenAI-loop
definitions (an Ollama model, a hosted model) — through `tddy-tools`'
`subagent_new_session` / `subagent_prompt` / `subagent_await` / `subagent_cancel` / `subagent_end`.
This PRD lets it open the same conversations with a **coding agent** that speaks ACP — Claude Code
(through `claude-agent-acp`), Codex (`codex-acp`), OpenCode (`opencode acp`), or `tddy-coder --acp`.
The daemon starts the agent as the session's own OS user, on the **main agent's worktree**, under the
same confinement the main agent has, and the main agent talks to it through the tools it already
knows.

## Background

Specialized agents are cheap helpers: a model, a prompt and a read/grep loop. A coding agent is a
peer: it plans, edits, runs tests and keeps its own context. The roster and the MCP tools were built
ACP-shaped from the start (`sessionId`, `stopReason`, `ContentBlock`), but every conversation is
still built as an OpenAI chat loop, and the roster refuses anything that is not an endpoint
definition. Harness profiles already name coding agents on an account and a model; they are just not
attachable.

## Proposed Changes

### What's Changing

- **Two kinds of roster entry.** Every roster entry, catalog row and persisted record carries a
  `kind`: `model` (today's definitions and registry assistants) or `coding_agent`. Records written
  before this change read as `model`.
- **Which coding agents are offered.** `ListSubagents` additionally lists, as `coding_agent` rows:
  - every harness profile (`<tddyhome>/agents/*.yaml` with `harness:`) whose harness speaks ACP
    (`claude-acp`, `codex-acp`, `opencode`), under its label, with its model and account;
  - every configured `allowed_agents` entry whose id is one of those harnesses, with the harness's
    default model.
  A row names its harness, so a picker can say what it is.
- **Attaching one.** `AttachSessionAgent` accepts a `coding_agent` row like any other. It is
  attached to the session's roster as `name@daemon`, revisioned and streamed like every entry.
  Coding agents replace no main-agent tools in this change.
- **Opening a conversation.** `subagent_new_session { agent }` on a coding-agent entry opens a
  conversation on the daemon (the in-jail process holds no definition for it). The daemon starts the
  agent process **as the session's OS user**, with the profile's provider account route applied,
  opens an ACP session on it, and keeps the process for the conversation's lifetime.
- **Where it works.** On the **main agent's worktree** — the same checkout, no isolation (a separate
  clone is a later node). In a managed-codebase session the subagent is managed too: its native
  file-system and shell tools are withdrawn and it is handed `tddy-tools --mcp` with the session's
  exec tools, which run against that worktree. A harness that cannot have its native tools withdrawn
  is not offered for managed sessions — attaching it is refused naming the harness.
- **Turns.** `subagent_prompt` sends one ACP prompt and returns the existing outcome shape:
  `stopReason` (`end_turn`, `max_turn_requests`, `cancelled`, …), `content` (the agent's text), `usage`
  (zero where the agent does not report it) and `messages` (one descriptor per tool call the turn
  made). The grace period, `responseId` and `subagent_await` work unchanged.
- **What a coding agent refuses.** `subagent_resume`'s `fromMessageId`, `correction`, `replacement`
  and `yieldConditions`, and `subagent_prompt`'s `yieldConditions` and `maxTurns`, are refused with a
  typed error naming the option — they act on an OpenAI-loop transcript the coding agent does not
  expose.
- **Permissions, for now.** A permission request from the coding agent is **denied** and the denial
  shows in the turn's `messages`. The next node of the stack hands it to the main agent instead.
- **Ending.** `subagent_cancel` cancels the running turn (`session/cancel`); `subagent_end` and a
  detach end the conversation and stop the process.
- **Web.** The create-session agent picker and the Add-agent picker list coding agents with a
  "coding agent · <harness>" badge; the Agents tab shows the same badge on a roster row.

### What's Staying the Same

- `model` entries — definitions and registry assistants — behave exactly as today, in the jail or on
  the daemon.
- The MCP tool names and their outcome JSON.
- Remote attach (`name@other-daemon`) for `model` entries. Remote coding agents are a later node.
- Harness profiles still start sessions from `ListAgents` as before.

## Impact Analysis

### Technical Impact

- Proto: `SessionAgentEntry.kind` / `.harness`, `SubagentInfo.kind` / `.harness` (new fields,
  defaulting to `model`).
- `tddy-changeset` `SessionAgentRecord.kind` / `.harness` (serde default).
- `tddy-discovery`: coding-agent catalog rows; a new `SubagentSession` implementation driving n1's
  `AcpDriver`.
- `tddy-session-agents`: `open_local` builds a coding-agent session for a `coding_agent` record;
  the process is spawned as the session's OS user.
- `tddy-tools`: none for routing (a coding-agent entry has no seed definition, so it already goes to
  the daemon); the refusals of loop-only options come from the session.
- `tddy-web`: a kind badge in two pickers and the Agents tab.

### User Impact

- A developer can attach "Codex" or "Claude Code" to a session and have the main agent delegate a
  task to it.

## Acceptance Criteria

- [ ] `ListSubagents` lists ACP harness profiles and ACP `allowed_agents` as `coding_agent` rows with their harness.
- [ ] Attaching a coding agent puts a `coding_agent` entry on the roster stream.
- [ ] A managed session refuses to attach a harness that cannot withdraw its native tools, naming it.
- [ ] `subagent_new_session` + `subagent_prompt` on a coding agent returns its text with `stopReason: end_turn`.
- [ ] A long coding-agent turn returns `{responseId, pending}` and `subagent_await` returns its outcome.
- [ ] The agent process runs as the session's OS user, with cwd on the session worktree (or, managed, with only `tddy-tools` exec tools against it).
- [ ] Loop-only options are refused with a typed error naming the option.
- [ ] A permission request is denied and recorded in the turn's `messages`.
- [ ] `subagent_cancel` ends a running turn as `cancelled`; `subagent_end` and detach stop the process.
- [ ] Records written before this change load as `model` entries.
- [ ] Both pickers and the Agents tab show the coding-agent badge.

## References

- [Session agent roster](../session-agent-roster.md), [Harness profiles](../harness-profiles.md)
- [Managed-codebase subagents](../../coder/managed-codebase-subagents.md)
- Stack parent: #720 (`AcpDriver`)
