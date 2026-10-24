# One ACP client driver for every ACP coding backend - PRD

**Date**: 2026-10-24
**PRD Type**: Technical Improvement
**Stack**: `#coagents` 1/8, branch `feature/coagents/acp-driver`

## Affected Features

- **Primary Feature**: [Codex ACP backend](../codex-acp-backend.md) — `codex-acp` runs on the shared driver; its `codex login` retry becomes a driver hook
- **Related Feature**: `claude-acp` (no feature doc of its own; named in [implementation-step.md](../implementation-step.md)) — runs on the shared driver
- **Related Feature**: [OpenCode ACP backend](../opencode-acp-backend.md) — `opencode` runs on the shared driver; its route files stay its own
- **Related Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — the driver is what the next node builds a coding-agent subagent on

## Summary

tddy speaks ACP to three coding agents — Claude (through the SDK-based `claude-agent-acp`
adapter), Codex (`codex-acp`) and OpenCode (`opencode acp`) — through three copies of one worker.
Each copy spawns the agent, connects over stdio, initializes, opens or loads a session and prompts
it; none of them can cancel a turn, none of them tells the agent which model to use, and none of
them hands the agent an MCP server. This PRD replaces the three copies with **one ACP client
driver** that does all of that, and moves the three backends onto it.

## Background

The `#coagents` stack lets a session's main agent open conversations with other coding agents. The
ACP path is the preferred transport for every agent that has an adapter, because ACP gives
structured tool-call updates, permission requests, cancellation and resumable sessions — the
things a subagent conversation needs. Today none of those are reachable through one interface:

| Today | `claude-acp` | `codex-acp` | `opencode` |
|---|---|---|---|
| requested `model` honoured | no | no (route only) | no (route only) |
| `session/cancel` | no | no | no |
| child killed when the backend is dropped | no | no | no |
| `mcp_servers` passed on `session/new` | no | no | no |
| resume across process restarts | no (in-memory map) | yes (`load_session`) | yes (`load_session`) |
| permission requests | auto `allow_once` | auto `allow_once` | auto `allow_once` |

The duplication is recorded as
[`docs/dev/todo/2026-10-22-acp-backends-share-one-worker.md`](../../../dev/todo/2026-10-22-acp-backends-share-one-worker.md)
and as two complexity records (`complexity-acp-run-acp-worker`,
`complexity-codex-acp-run-codex-acp-worker`).

## Proposed Changes

### What's Changing

- **One driver.** A single ACP client driver owns the child process and its connection, exposes
  *open a session* (fresh or by id), *prompt* (streaming `session/update`s to a sink), *cancel*,
  *set the model* and *list models*, and kills the child when it is dropped.
- **The model is honoured.** A requested model is applied to the session (`session/set_model`) when
  the agent advertises it; a model the agent does not offer is an error naming the offered ones,
  never a silent fallback to the agent's default.
- **Cancel works.** Cancelling a running turn sends `session/cancel`, and the turn ends with
  `stop_reason: cancelled`.
- **MCP servers can be handed to the agent.** A session can be opened with stdio MCP servers (the
  next node hands it `tddy-tools --mcp`).
- **Permission requests go to a handler.** The driver asks an injected permission handler instead of
  answering `allow_once` itself. The three workflow backends keep today's behaviour by installing a
  handler that allows once — a subagent installs a different one later in the stack.
- **Resume is uniform.** `claude-acp` resumes by `session/load` like the other two, so a resumed
  session survives a backend restart when the agent supports loading.
- **The three backends keep their own quirks as hooks**: Codex's one `codex login` retry on an
  auth-like error, OpenCode's route configuration files, Claude's route environment.

### What's Staying the Same

- Backend ids (`claude-acp`, `codex-acp`, `opencode`), their binaries and their env overrides.
- Provider routes and their secret handling.
- How workflow recipes call a backend (`CodingBackend::invoke`).
- Permission behaviour of the workflow backends (allow once).

## Impact Analysis

### Technical Impact

- `tddy-agent-backend`: a new driver module; `acp.rs`, `codex_acp.rs`, `opencode_acp.rs` shrink to
  their per-agent hooks; `acp_updates.rs` becomes the driver's update mapping.
- `tddy-acp-stub`: records what it was asked (session model, MCP servers, cancel) so tests can
  observe the client, and can hold a turn open until cancelled.
- No proto, RPC or web change.

### User Impact

- Choosing a model for an ACP backend now takes effect.
- Stopping an ACP-backed session stops its agent's turn instead of leaving it running.

## Acceptance Criteria

- [ ] A prompt through each of the three ACP backends against `tddy-acp-stub` returns the stub's text, as today.
- [ ] A requested model is applied to the session; an unknown model fails naming the offered models.
- [ ] Cancelling a running turn ends it with `cancelled` and the stub records the `session/cancel`.
- [ ] Dropping a backend terminates its agent process.
- [ ] A session opened with an MCP server passes it on `session/new`.
- [ ] A permission request reaches the installed handler, and its decision is what the agent receives.
- [ ] `claude-acp` resumes by `session/load`.
- [ ] Codex's `codex login` retry still runs once on an auth-like error.
- [ ] The two complexity records are re-measured and closed; the TODO is deleted at wrap.

## References

- [`docs/dev/todo/2026-10-22-acp-backends-share-one-worker.md`](../../../dev/todo/2026-10-22-acp-backends-share-one-worker.md)
- [Managed-codebase subagents](../managed-codebase-subagents.md)
