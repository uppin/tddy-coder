# 2026-11-04 — The OpenCode ACP route config stays inline; the route file has no session owner

**Category:** Deferred from `acp-agents` (#800, `#agent-tools 11/12`)

**Source:** `docs/dev/changesets/2026-11-04-agent-tools-acp-agents.md`, § Deferred

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

## Not closed by `#agent-tools 27/28` (`acp-subagent-leftovers`, #835)

Pass 2 found that a route file the **daemon** writes cannot be read by an agent started as the session's
OS user: the adapter writes it into the daemon's own temp directory, 0700/0600, and a `setpriv` or
supervisor-relayed agent runs as another user. Inline `OPENCODE_CONFIG_CONTENT` works because the
environment crosses the switch. The developer chose (2026-11-07) a route file **owned by the session
user** — chown when the daemon is privileged, written by the supervisor for a relayed launch — as a
**follow-up stack node of its own**, which owns this record. #835 keeps the inline route and only adds
the hooks plumbing (`LaunchedAgent`, `CodingAgentSession::start`'s hooks) that node will use.
