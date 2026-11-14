# 2026-11-10 — Start a session at an effort level

- A session can be started at a reasoning effort level beside its model; the agent's spec declares which levels it takes. Claude takes `low`, `medium`, `high`, `xhigh` and `max`.
- An unknown level, or any level for an agent that takes none, is refused naming why.
- The level survives a resume, sandboxed sessions included.
- The New-session form shows an **Effort** picker for agents with levels, and an orchestrator's `StartSession` takes `effort`; `DescribeSessionStartOptions` lists each agent's levels.

See [Claude CLI session § Effort](../claude-cli-session.md#effort), [Project concept § Start options](../project-concept.md#start-options) and [Session drawer § Fields](../../web/session-drawer.md#fields).
