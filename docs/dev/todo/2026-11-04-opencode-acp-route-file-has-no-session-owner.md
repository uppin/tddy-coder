# 2026-11-04 — The OpenCode ACP route config stays inline; the route file has no session owner

**Category:** Deferred from `acp-agents` (#800, `#agent-tools 11/12`)

**Source:** `docs/dev/1-WIP/2026-10-31-agent-tools-acp-agents.md`, § Decisions & Trade-offs

## What remains

The changeset planned the OpenCode ACP route config as a declared file for the daemon launcher too,
written by the `opencode-route-file` hooks adapter, in place of the inline
`OPENCODE_CONFIG_CONTENT`. The daemon launcher still passes it inline (`coding_agent_launcher.rs`,
marked `TODO(acp-agents)` at the route-file site).

## Why it stays

`CodingAgentSession::start` spawns the agent with `NoHooks`, and nothing owns the route file's
lifetime. Writing the file there would have it removed at once, so the daemon keeps the inline
config. The `opencode-route-file` adapter is still used by the `tddy-coder` ACP path.

## Sweep work

Give the route file a session-scoped owner in `CodingAgentSession::start`, so the daemon launcher
writes it the way the adapter does, then drop the inline `OPENCODE_CONFIG_CONTENT` for ACP.
